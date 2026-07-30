use soroban_sdk::Env;

pub fn random_in_range(env: &Env, max: u32) -> u32 {
    let salt = env.prng().gen::<u64>();
    let sequence = env.ledger().sequence();
    let seed = salt.wrapping_add(sequence as u64);
    if max == 0 { return 0; }
    (seed % (max as u64)) as u32
}

pub fn shuffle_positions(env: &Env, n: u32) -> soroban_sdk::Vec<u32> {
    let mut shuffled = soroban_sdk::Vec::new(env);
    // Track used positions in a Vec; linear scan is fine for small circle sizes
    let mut used: soroban_sdk::Vec<u32> = soroban_sdk::Vec::new(env);

    for _ in 0..n {
        // If all positions are already chosen, stop early
        if used.len() >= n {
            break;
        }
        loop {
            let pos = random_in_range(env, n);
            if !used.contains(&pos) {
                shuffled.push_back(pos);
                used.push_back(pos);
                break;
            }
        }
    }
    shuffled
}
