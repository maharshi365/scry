# Re-download the pinned official originals and regenerate Rune-compatible bodies.
$ErrorActionPreference = 'Stop'
$commit = '0ea8780434e2b4527dc860b18ff94883d080b59c'
$base = "https://raw.githubusercontent.com/lucide-icons/lucide/$commit"
$names = @(
    'download', 'trash-2', 'play', 'pause', 'rewind', 'fast-forward',
    'volume-2', 'volume-x', 'captions', 'audio-lines', 'square', 'maximize',
    'minimize', 'arrow-left', 'arrow-right', 'chevron-down', 'refresh-cw',
    'x', 'check', 'plus',
    'layers', 'flag', 'sun', 'shield', 'scan-eye', 'graduation-cap',
    'repeat-2', 'moon', 'briefcase-business', 'trophy', 'clock-3',
    'chart-no-axes-combined', 'hourglass'
)
$upstream = Join-Path $PSScriptRoot 'lucide'
$bodies = Join-Path $PSScriptRoot 'rune'
New-Item -ItemType Directory -Force $upstream, $bodies | Out-Null
foreach ($name in $names) {
    $original = Join-Path $upstream "$name.svg"
    Invoke-WebRequest "$base/icons/$name.svg" -OutFile $original
    $svg = [IO.File]::ReadAllText($original)
    $rootEnd = $svg.IndexOf('>')
    $root = $svg.Substring(0, $rootEnd)
    $styles = @('fill', 'stroke', 'stroke-width', 'stroke-linecap', 'stroke-linejoin') | ForEach-Object {
        $match = [regex]::Match($root, "\b$_=`"[^`"]*`"")
        if (!$match.Success) { throw "Missing root style $_ in $name" }
        $match.Value
    }
    $style = $styles -join ' '
    $inner = $svg.Substring($rootEnd + 1, $svg.LastIndexOf('</svg>') - $rootEnd - 1)
    $elements = foreach ($line in ($inner.Trim() -split "`n")) {
        $element = $line.Trim()
        if ($element -notmatch '^<(path|rect|circle|ellipse|line) ' -or !$element.EndsWith(' />')) {
            throw "Unsupported Rune element in ${name}: $element"
        }
        $element.Substring(0, $element.Length - 3) + " $style />"
    }
    $adapted = '<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24" viewBox="0 0 24 24">' + "`n" + ($elements -join "`n") + "`n</svg>`n"
    [IO.File]::WriteAllText((Join-Path $bodies "$name.svg"), $adapted, [Text.UTF8Encoding]::new($false))
}
