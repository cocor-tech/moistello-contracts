$text = Get-Content -Path 'packages/treasury/src/test.rs' -Raw

$setup_old = 'fn setup(env: &Env) -> (TreasuryClient, Address) {
    env.mock_all_auths();
    let contract_id = env.register(Treasury, ());
    let client = TreasuryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    client.init(&admin);
    (client, admin)
}'
$setup_new = 'fn setup(env: &Env) -> (TreasuryClient, Address, Address) {
    env.mock_all_auths();
    let contract_id = env.register(Treasury, ());
    let client = TreasuryClient::new(env, &contract_id);
    let admin = Address::generate(env);
    let token_admin = Address::generate(env);
    let token = env.register_stellar_asset_contract(token_admin);
    client.init(&admin, &token);
    (client, admin, token)
}

fn mint_tokens(env: &Env, token: &Address, recipient: &Address, amount: i128) {
    let token_client = soroban_sdk::token::StellarAssetClient::new(env, token);
    token_client.mint(recipient, &amount);
}'
$text = $text.Replace($setup_old, $setup_new)

$text = $text -replace '\(client, _admin\) = setup\(&env\)', '(client, _admin, token) = setup(&env)'
$text = $text -replace '\(client, admin\) = setup\(&env\)', '(client, admin, token) = setup(&env)'

$text = $text -replace 'client\.deposit\(&from, &1000i128, &circle_id\);', 'mint_tokens(&env, &token, &from, 1000i128);
    client.deposit(&from, &1000i128, &circle_id);'
$text = $text -replace 'client\.deposit\(&from, &500i128, &circle_id\);', 'mint_tokens(&env, &token, &from, 500i128);
    client.deposit(&from, &500i128, &circle_id);'
$text = $text -replace 'client\.deposit\(&from, &2000i128, &circle_id\);', 'mint_tokens(&env, &token, &from, 2000i128);
    client.deposit(&from, &2000i128, &circle_id);'
$text = $text -replace 'client\.deposit\(&from, &2000i128, &c2\);', 'mint_tokens(&env, &token, &from, 2000i128);
    client.deposit(&from, &2000i128, &c2);'

Set-Content -Path 'packages/treasury/src/test.rs' -Value $text -NoNewline
