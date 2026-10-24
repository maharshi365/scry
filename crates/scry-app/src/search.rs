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
    if input.chars().count() > 256 {
        return Err("Search is limited to 256 characters");
    }
    Ok(input.split_whitespace().collect::<Vec<_>>().join(" "))
}

pub fn plan(addons: Vec<Addon>) -> Result<Vec<Request>, &'static str> {
    if addons.len() > 32 {
        return Err("At most 32 installed add-ons are supported");
    }
    let mut requests = Vec::new();
    for (addon_index, addon) in addons.into_iter().enumerate() {
        for (catalog_index, catalog) in addon.manifest.catalogs.iter().enumerate() {
            if matches!(catalog.kind.as_str(), "movie" | "series")
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

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
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
    batches: BTreeMap<usize, Vec<SearchResult>>,
    warnings: BTreeMap<usize, String>,
}

impl Results {
    pub fn record(&mut self, index: usize, request: &Request, result: Result<Vec<Meta>, ()>) {
        match result {
            Ok(metas) => {
                self.batches.insert(
                    index,
                    metas
                        .into_iter()
                        .filter(|meta| {
                            meta.kind == request.catalog.kind && !meta.id.trim().is_empty()
                        })
                        .take(RESULT_LIMIT)
                        .map(|meta| SearchResult {
                            meta,
                            addon: request.addon.clone(),
                        })
                        .collect(),
                );
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
            .flatten()
            .filter(|result| seen.insert((result.meta.kind.clone(), result.meta.id.clone())))
            .map(|result| (normalize(&result.meta.name), result))
            .collect();
        // Stable ties preserve catalog and provider response order.
        results.sort_by_key(|(title, _)| rank(title, &query));
        let results = results
            .into_iter()
            .take(RESULT_LIMIT)
            .map(|(_, result)| result.clone())
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
    fn plans_all_movie_and_series_search_catalogs_not_browse_or_required_filters() {
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
            ["movie", "series"]
        );
        assert_eq!(requests[0].catalog.id, "movie");
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
