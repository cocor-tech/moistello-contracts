use soroban_sdk::{contract, contractimpl, Address, Bytes, BytesN, Env, String};

#[contract]
pub struct CircleFactoryContract;

#[contractimpl]
impl CircleFactoryContract {
    pub fn deploy_circle(
        env: Env,
        organizer: Address,
        name: String,
        count: u32,
    ) -> Address {
        organizer.require_auth();

        // FIX: Combine deployment count with cryptographic hash of organizer and name for unique salt generation
        let mut hasher_input = Bytes::new(&env);
        hasher_input.append(&organizer.to_xdr(&env));
        hasher_input.append(&name.to_xdr(&env));
        hasher_input.append(&count.to_be_bytes());

        let hashed_salt = env.crypto().sha256(&hasher_input);
        let salt = BytesN::from_array(&env, &hashed_salt.to_array());

        let deployed_address = env
            .deployer()
            .with_current_contract(salt)
            .deploy_v2(organizer, ());

        deployed_address
    }
}