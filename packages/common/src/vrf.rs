use soroban_sdk::{Env, Map};
pub fn random_in_range(env: &Env, max: u32) -> u32 {
    let salt = env.prng().gen::<u64>();
    let sequence = env.ledger().sequence();
    let seed = salt.wrapping_add(sequence as u64);
    if max == 0 { return 0; }
    (seed % (max as u64)) as u32
}
pub fn shuffle_positions(env: &Env, n: u32) -> soroban_sdk::Vec<u32> {
    let mut shuffled = soroban_sdk::Vec::new(env);
    // Use Map<u32, bool> as a set — soroban_sdk has no dedicated Set type.
    let mut used: Map<u32, bool> = Map::new(env);
    let mut remaining = n;

    for _ in 0..n {
        loop {
            let pos = random_in_range(env, n);
            if !used.contains_key(pos) {
                shuffled.push_back(pos);
                used.set(pos, true);
                break;
            }
            remaining = remaining.saturating_sub(1);
            if remaining == 0 { break; }
        }
    }
    shuffled
}
