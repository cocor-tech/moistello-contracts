import re

with open('uxupgrade.md', 'r', encoding='utf-8') as f:
    text = f.read()

text = text.replace('# Moistello — Governance & Reputation UX Upgrade Plan', '# Moistello — Reputation UX Upgrade Plan')
text = text.replace('fully-governed, ', '')
text = text.replace(' | Governance Votes |', ' |')
text = re.sub(r' \| \d+x \|', ' |', text)

text = re.sub(r'## 2\. Governance System.*?---', '---', text, flags=re.DOTALL)

text = re.sub(r'### Phase B — Governance Contract \(3 days\).*?### Phase C — Circle Integration \(1 day\)', '### Phase B — Circle Integration (1 day)', text, flags=re.DOTALL)

text = re.sub(r'- \[ \] Add governance voting UI to /governance route\n', '', text)
text = re.sub(r'- \[ \] Add proposal creation form with action builder\n', '', text)
text = re.sub(r'- \[ \] Add proposal listing with live vote counts\n', '', text)

text = re.sub(r'- \[ \] Generate Go bindings for new governance contract\n', '', text)
text = re.sub(r'- \[ \] Integration tests: create proposal ? vote ? execute\n', '', text)
text = re.sub(r'- \[ \] End-to-end governance test on testnet\n', '', text)

text = re.sub(r'\| Whale domination.*?\|\n', '', text)
text = re.sub(r'\| Flash loan.*?\|\n', '', text)
text = re.sub(r'\| Timelock bypass.*?\|\n', '', text)
text = re.sub(r'\| Proposal spam.*?\|\n', '', text)

text = re.sub(r'  ? 3x Governance Vote Power.*?\n', '', text)
text = re.sub(r'  ?? 5x Governance Vote Power.*?\n', '', text)

text = re.sub(r' — ?? Governance: \d+x votes', '', text)

with open('uxupgrade.md', 'w', encoding='utf-8') as f:
    f.write(text)
