$text = Get-Content -Path 'uxupgrade.md' -Raw

$text = $text -replace '# Moistello — Governance & Reputation UX Upgrade Plan', '# Moistello — Reputation UX Upgrade Plan'
$text = $text -replace 'fully-governed, ', ''
$text = $text -replace ' \| Governance Votes \|', ' |'
$text = $text -replace ' \| \d+x \|', ' |'

$text = $text -replace '(?s)## 2\. Governance System.*?---', '---'

$text = $text -replace '(?s)### Phase B — Governance Contract \(3 days\).*?### Phase C — Circle Integration \(1 day\)', '### Phase B — Circle Integration (1 day)'

$text = $text -replace '- \[ \] Add governance voting UI to /governance route\r?\n', ''
$text = $text -replace '- \[ \] Add proposal creation form with action builder\r?\n', ''
$text = $text -replace '- \[ \] Add proposal listing with live vote counts\r?\n', ''

$text = $text -replace '- \[ \] Generate Go bindings for new governance contract\r?\n', ''
$text = $text -replace '- \[ \] Integration tests: create proposal ? vote ? execute\r?\n', ''
$text = $text -replace '- \[ \] End-to-end governance test on testnet\r?\n', ''

$text = $text -replace '\| Whale domination.*?\|\r?\n', ''
$text = $text -replace '\| Flash loan.*?\|\r?\n', ''
$text = $text -replace '\| Timelock bypass.*?\|\r?\n', ''
$text = $text -replace '\| Proposal spam.*?\|\r?\n', ''

$text = $text -replace '  ? 3x Governance Vote Power.*?\r?\n', ''
$text = $text -replace '  ?? 5x Governance Vote Power.*?\r?\n', ''

$text = $text -replace ' — ?? Governance: \d+x votes', ''

Set-Content -Path 'uxupgrade.md' -Value $text -NoNewline
