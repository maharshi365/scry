//! Search planning and deterministic merging, independent of network completion order.
use scry_addons::{Addon, Catalog, Meta};
use std::collections::{BTreeMap, HashSet};

pub const CONCURRENCY: usize = 4;
const RESULT_LIMIT: usize = 100;
const REQUEST_LIMIT: usize = 64;

#[derive(Clone, Debug)]
pub struct SearchResult {
    pub meta: Meta,
    pub addon: Addon,
}

pub struct Request {
    pub addon: Addon,
    pub catalog: Catalog,
    pub addon_index: usize,
    pub catalog_index: usize,
}

pub fn query(input: &str) -> Result<String, &'static str> {
    if input.chars().nth(256).is_some() {
        return Err("Search is limited to 256 characters");
    }
    Ok(collapse_whitespace(input))
}

pub fn plan(addons: Vec<Addon>) -> Result<Vec<Request>, &'static str> {
    if addons.len() > 32 {
        return Err("At most 32 installed add-ons are supported");
    }
    let mut requests = Vec::new();
    for (addon_index, addon) in addons.into_iter().enumerate() {
        if addon.disabled {
            continue;
        }
        for (catalog_index, catalog) in addon.manifest.catalogs.iter().enumerate() {
            if !catalog.kind.trim().is_empty()
                && catalog.extra.iter().any(|extra| extra.name == "search")
                && !catalog
                    .extra
                    .iter()
                    .any(|extra| extra.is_required && extra.name != "search")
            {
                if requests.len() == REQUEST_LIMIT {
                    return Err("Too many searchable catalogs; disable an add-on and try again");
                }
                requests.push(Request {
                    addon: addon.clone(),
                    catalog: catalog.clone(),
                    addon_index,
                    catalog_index,
                });
            }
        }
    }
    Ok(requests)
}

fn collapse_whitespace(value: &str) -> String {
    let mut result = String::with_capacity(value.len());
    for word in value.split_whitespace() {
        if !result.is_empty() {
            result.push(' ');
        }
        result.push_str(word);
    }
    result
}

fn normalize(value: &str) -> String {
    collapse_whitespace(value).to_lowercase()
}

fn rank(title: &str, query: &str) -> u8 {
    if title == query {
        0
    } else if title.starts_with(query) {
        1
    } else if title.contains(query) {
        2
    } else {
        3
    }
}

#[derive(Default)]
pub struct Results {
    // Plan order, not response order, determines which provider owns a duplicate.
    batches: BTreeMap<usize, (Addon, Vec<Meta>)>,
    warnings: BTreeMap<usize, String>,
}

impl Results {
    pub fn record(&mut self, index: usize, request: &Request, result: Result<Vec<Meta>, ()>) {
        match result {
            Ok(metas) => {
                // Do not retain an oversized provider allocation in a bounded batch.
                let mut retained = Vec::with_capacity(metas.len().min(RESULT_LIMIT));
                retained.extend(
                    metas
                        .into_iter()
                        .filter(|meta| {
                            matches!(meta.kind.as_str(), "movie" | "series")
                                && !meta.id.trim().is_empty()
                        })
                        .take(RESULT_LIMIT),
                );
                self.batches
                    .insert(index, (request.addon.clone(), retained));
            }
            Err(()) => {
                // Do not echo provider-controlled names, errors, URLs, or the query.
                self.warnings.insert(
                    index,
                    format!(
                        "Add-on {}, catalog {}: search failed or timed out",
                        request.addon_index + 1,
                        request.catalog_index + 1,
                    ),
                );
            }
        }
    }

    pub fn snapshot(&self, query: &str) -> (Vec<SearchResult>, Vec<String>) {
        let query = normalize(query);
        let mut seen = HashSet::new();
        let mut results: Vec<_> = self
            .batches
            .values()
            .flat_map(|(addon, metas)| metas.iter().map(move |meta| (addon, meta)))
            .filter(|(_, meta)| seen.insert((meta.kind.as_str(), meta.id.as_str())))
            .map(|(addon, meta)| (rank(&normalize(&meta.name), &query), addon, meta))
            .collect();
        // Stable ties preserve catalog and provider response order.
        results.sort_by_key(|(rank, _, _)| *rank);
        let results = results
            .into_iter()
            .take(RESULT_LIMIT)
            .map(|(_, addon, meta)| SearchResult {
                meta: meta.clone(),
                addon: addon.clone(),
            })
            .collect();
        (results, self.warnings.values().cloned().collect())
    }
}

#[cfg(test)]
#[expect(
    clippy::unwrap_used,
    reason = "Search fixtures and asserted successful results must fail the test on error"
)]
mod tests {
    use super::*;
    use serde_json::json;

    fn addon(id: &str, catalogs: serde_json::Value) -> Addon {
        let mut value = json!({
            "manifest_url": "https://example.com/private/manifest.json",
            "manifest": {"id": id, "name": id}
        });
        value["manifest"]["catalogs"] = catalogs;
        serde_json::from_value(value).unwrap()
    }

    fn searchable(kind: &str) -> serde_json::Value {
        json!({"id":kind, "type":kind, "extra":[{"name":"search"}]})
    }

    fn meta(kind: &str, id: &str, name: &str) -> Meta {
        serde_json::from_value(json!({"id":id, "type":kind, "name":name})).unwrap()
    }

    #[test]
    fn disabled_providers_are_not_searched() {
        let mut disabled = addon("disabled", json!([searchable("movie")]));
        disabled.disabled = true;
        let requests = plan(vec![
            disabled,
            addon("enabled", json!([searchable("movie")])),
        ])
        .unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].addon.manifest.id, "enabled");
    }

    #[test]
    fn plans_all_category_search_catalogs_not_browse_or_required_filters() {
        let requests = plan(vec![addon(
            "provider",
            json!([
                {"id":"browse", "type":"movie"},
                searchable("movie"), searchable("series"), searchable("channel"),
                {"id":"filtered", "type":"movie", "extra":[
                    {"name":"search"}, {"name":"genre", "isRequired":true}]}
            ]),
        )])
        .unwrap();
        assert_eq!(
            requests
                .iter()
                .map(|r| r.catalog.kind.as_str())
                .collect::<Vec<_>>(),
            ["movie", "series", "channel"]
        );
        assert_eq!(requests[0].catalog.id, "movie");
    }

    #[test]
    fn category_search_accepts_playable_types_and_preserves_provider_ids() {
        for category in ["anime", "documentary"] {
            let request = plan(vec![addon("provider", json!([searchable(category)]))])
                .unwrap()
                .pop()
                .unwrap();
            let mut results = Results::default();
            results.record(
                0,
                &request,
                Ok(vec![
                    meta("series", "kitsu:11", "Naruto"),
                    meta("movie", "kitsu:2245", "Naruto movie"),
                    meta("channel", "live", "Unsupported"),
                    meta("movie", " ", "Missing identity"),
                ]),
            );
            let (found, warnings) = results.snapshot("Naruto");
            assert!(warnings.is_empty());
            assert_eq!(
                found
                    .iter()
                    .map(|r| (r.meta.kind.as_str(), r.meta.id.as_str()))
                    .collect::<Vec<_>>(),
                [("series", "kitsu:11"), ("movie", "kitsu:2245")]
            );
            assert!(found.iter().all(|r| r.addon.manifest.id == "provider"));
        }
    }

    #[test]
    fn deduplication_preserves_type_and_preferred_provider_metadata_despite_completion_order() {
        let mut requests = plan(vec![
            addon("first", json!([searchable("movie"), searchable("series")])),
            addon("second", json!([searchable("movie")])),
        ])
        .unwrap()
        .into_iter()
        .enumerate()
        .collect::<Vec<_>>();
        let mut results = Results::default();
        let (index, request) = requests.pop().unwrap();
        results.record(
            index,
            &request,
            Ok(vec![meta("movie", "same", "Other title")]),
        );
        let (index, request) = requests.pop().unwrap();
        results.record(index, &request, Ok(vec![meta("series", "same", "Series")]));
        let (index, request) = requests.pop().unwrap();
        results.record(
            index,
            &request,
            Ok(vec![meta("movie", "same", "Original title")]),
        );
        let (found, _) = results.snapshot("title");
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].meta.name, "Original title");
        assert_eq!(found[0].addon.manifest.id, "first");
        assert_eq!(found[1].meta.kind, "series");
    }

    #[test]
    fn ranks_normalized_unicode_titles_and_keeps_success_after_failure() {
        let requests = plan(vec![addon(
            "provider",
            json!([searchable("movie"), searchable("series")]),
        )])
        .unwrap();
        let mut results = Results::default();
        for (index, request) in requests.into_iter().enumerate() {
            if index == 0 {
                results.record(
                    index,
                    &request,
                    Ok(vec![
                        meta("movie", "1", "Other"),
                        meta("movie", "2", "The ÉTOILE"),
                        meta("movie", "3", "Étoile rising"),
                        meta("movie", "4", "  ÉTOILE \t"),
                    ]),
                );
            } else {
                results.record(index, &request, Err(()));
            }
        }
        let (found, warnings) = results.snapshot("étoile");
        assert_eq!(
            found.iter().map(|r| r.meta.id.as_str()).collect::<Vec<_>>(),
            ["4", "3", "2", "1"]
        );
        assert_eq!(
            warnings,
            ["Add-on 1, catalog 2: search failed or timed out"]
        );
    }

    #[test]
    fn equal_rank_results_keep_provider_order_after_deduplication() {
        let requests = plan(vec![
            addon("first", json!([searchable("movie")])),
            addon("second", json!([searchable("movie")])),
        ])
        .unwrap();
        let mut results = Results::default();
        results.record(
            1,
            &requests[1],
            Ok(vec![
                meta("movie", "duplicate", "Query replacement"),
                meta("movie", "second", "Query second"),
            ]),
        );
        results.record(
            0,
            &requests[0],
            Ok(vec![
                meta("movie", "duplicate", "Query original"),
                meta("movie", "first", "Query first"),
            ]),
        );
        let (found, _) = results.snapshot("query");
        assert_eq!(
            found.iter().map(|r| r.meta.id.as_str()).collect::<Vec<_>>(),
            ["duplicate", "first", "second"]
        );
        assert_eq!(found[0].meta.name, "Query original");
        assert_eq!(found[0].addon.manifest.id, "first");
    }

    #[test]
    fn retained_batches_do_not_keep_oversized_provider_buffers() {
        let request = plan(vec![addon("provider", json!([searchable("movie")]))])
            .unwrap()
            .pop()
            .unwrap();
        let mut metas = Vec::with_capacity(4096);
        metas.extend((0..150).map(|index| meta("movie", &index.to_string(), "Title")));
        let mut results = Results::default();
        results.record(0, &request, Ok(metas));
        let (_, retained) = results.batches.get(&0).unwrap();
        assert_eq!(retained.len(), RESULT_LIMIT);
        assert!(
            retained.capacity() <= RESULT_LIMIT,
            "bounded search batches must release oversized provider buffers"
        );
    }

    #[test]
    fn snapshots_keep_owned_results_when_a_batch_is_replaced() {
        let request = plan(vec![addon("provider", json!([searchable("movie")]))])
            .unwrap()
            .pop()
            .unwrap();
        let mut results = Results::default();
        results.record(0, &request, Ok(vec![meta("movie", "first", "First title")]));
        let (mut first, _) = results.snapshot("title");
        first[0].addon.manifest.name = "Changed snapshot".into();
        let (unchanged, _) = results.snapshot("title");
        assert_eq!(unchanged[0].addon.manifest.name, "provider");

        results.record(
            0,
            &request,
            Ok(vec![meta("movie", "second", "Second title")]),
        );
        let (second, _) = results.snapshot("title");
        assert_eq!(first[0].meta.id, "first");
        assert_eq!(second.len(), 1);
        assert_eq!(second[0].meta.id, "second");
        assert_eq!(second[0].addon.manifest.id, "provider");
    }

    #[test]
    fn whitespace_normalization_preserves_unicode_case_rules() {
        assert_eq!(
            query("\u{2003}ΟΣ\t İ\u{a0}Straße \n").unwrap(),
            "ΟΣ İ Straße"
        );
        assert_eq!(
            normalize("\u{2003}ΟΣ\t İ\u{a0}Straße \n"),
            "ος i\u{307} straße"
        );
    }

    #[test]
    fn bounds_results_and_validates_query_by_characters() {
        assert!(query(&"é".repeat(256)).is_ok());
        assert!(query(&"é".repeat(257)).is_err());
        assert_eq!(query(" \t \n").unwrap(), "");
        assert_eq!(query("a\t b").unwrap(), "a b");
        let request = plan(vec![addon("provider", json!([searchable("movie")]))])
            .unwrap()
            .pop()
            .unwrap();
        let mut results = Results::default();
        results.record(
            0,
            &request,
            Ok((0..150)
                .map(|i| meta("movie", &i.to_string(), "Title"))
                .collect()),
        );
        assert_eq!(results.snapshot("title").0.len(), 100);
    }
}
