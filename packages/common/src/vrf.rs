use soroban_sdk::{Env, Set};
pub fn random_in_range(env: &Env, max: u32) -> u32 {
    let salt = env.prng().gen::<u64>();
    let sequence = env.ledger().sequence();
    let seed = salt.wrapping_add(sequence as u64);
    if max == 0 { return 0; }
    (seed % (max as u64)) as u32
}
pub fn shuffle_positions(env: &Env, n: u32) -> soroban_sdk::Vec<u32> {
    let mut shuffled = soroban_sdk::Vec::new(env);
    let mut used: Set<u32> = Set::new(env);
    let mut remaining = n;

    for _ in 0..n {
        loop {
            let pos = random_in_range(env, n);
            if !used.contains(&pos) {
                shuffled.push_back(pos);
                used.insert(pos);
                break;
            }
            remaining = remaining.saturating_sub(1);
            if remaining == 0 { break; }
        }
    }
    shuffled
}
