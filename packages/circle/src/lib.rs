#![cfg_attr(not(test), no_std)]
// Deny unused imports at the crate level so stale imports (e.g. a previously
// re-exported `load_round_details` from payout.rs) are caught at compile time
// rather than silently emitting warnings that can be overlooked.
#![deny(unused_imports)]

#[cfg(test)]
#[path = "../../../circle.rs"]
pub mod circle_rs;
mod contract;
mod oracle;
mod payout;
#[cfg(test)]
mod test;
#[cfg(test)]
mod tests;
mod types;

use soroban_sdk::{contract, contractimpl, Address, BytesN, Env, Vec as SorobanVec};
use types::{
    Circle as CircleState, CircleConfig, CircleError, Contribution, DisputeResolutionRecord,
    Member, Referral, RoundFeeLedger, Streak,
};

pub use types::CircleError as ExportedCircleError;

type AddressList = SorobanVec<Address>;
type MemberList = SorobanVec<Member>;
type ContributionList = SorobanVec<Contribution>;
type ReferralList = SorobanVec<Referral>;
type StreakList = SorobanVec<Streak>;
type TopContributorsList = SorobanVec<(Address, i128)>;
type ContractResult<T = ()> = Result<T, CircleError>;

/// # Circle Contract
///
/// A rotating savings and credit association (ROSCA) implementation on Soroban.
/// Members contribute a fixed amount each round, and one member receives the
/// pool per round via random selection, fixed order, auction, or vote.
///
/// ## Example: Initialize a circle
///
/// ```no_run
/// use soroban_sdk::{Address, Env, String};
///
/// let env = Env::default();
/// let admin = Address::from_string(&String::from_str(&env, "GAADMIN..."));
/// let factory = Address::from_string(&String::from_str(&env, "GAFACTORY..."));
/// let token = Address::from_string(&String::from_str(&env, "GATOKEN..."));
/// let organizer = Address::from_string(&String::from_str(&env, "GAORG..."));
///
/// // CircleConfig would be constructed here in a real deployment
/// // The factory deploys the contract and calls __constructor
/// ```
///
/// ## Example: Join a circle
///
/// ```no_run
/// use soroban_sdk::{Address, Env, String};
/// use circle::CircleClient;
///
/// let env = Env::default();
/// let circle_id = Address::from_string(&String::from_str(&env, "GACIRCLE..."));
/// let member = Address::from_string(&String::from_str(&env, "GAMEMBER..."));
///
/// let client = CircleClient::new(&env, &circle_id);
/// client.join(&member);
/// ```
///
/// ## Example: Contribute to a round
///
/// ```no_run
/// use soroban_sdk::{Address, Env, String};
/// use circle::CircleClient;
///
/// let env = Env::default();
/// let circle_id = Address::from_string(&String::from_str(&env, "GACIRCLE..."));
/// let member = Address::from_string(&String::from_str(&env, "GAMEMBER..."));
///
/// let client = CircleClient::new(&env, &circle_id);
/// client.contribute(&member, &100_0000000, &1); // amount, round
/// ```
///
/// ## Example: Trigger payout for a round
///
/// ```no_run
/// use soroban_sdk::{Address, Env, String};
/// use circle::CircleClient;
///
/// let env = Env::default();
/// let circle_id = Address::from_string(&String::from_str(&env, "GACIRCLE..."));
/// let organizer = Address::from_string(&String::from_str(&env, "GAORG..."));
///
/// let client = CircleClient::new(&env, &circle_id);
/// client.trigger_payout(&organizer, &1); // round
/// ```
///
/// ## Example: Claim referral bonus
///
/// ```no_run
/// use soroban_sdk::{Address, Env, String};
/// use circle::CircleClient;
///
/// let env = Env::default();
/// let circle_id = Address::from_string(&String::from_str(&env, "GACIRCLE..."));
/// let referrer = Address::from_string(&String::from_str(&env, "GAREFERRER..."));
///
/// let client = CircleClient::new(&env, &circle_id);
/// client.claim_referral_bonus(&referrer);
/// ```
///
/// ## Example: Claim streak bonus
///
/// ```no_run
/// use soroban_sdk::{Address, Env, String};
/// use circle::CircleClient;
///
/// let env = Env::default();
/// let circle_id = Address::from_string(&String::from_str(&env, "GACIRCLE..."));
/// let member = Address::from_string(&String::from_str(&env, "GAMEMBER..."));
///
/// let client = CircleClient::new(&env, &circle_id);
/// client.claim_streak_bonus(&member);
/// ```
#[contract]
pub struct Circle;

#[contractimpl]
impl Circle {
    // =========================================================================
    // Lifecycle & Membership
    // =========================================================================

    /// Initializes a new circle instance upon deployment by the factory.
    pub fn __constructor(
        env: Env,
        admin: Address,
        factory: Address,
        config: CircleConfig,
    ) -> ContractResult {
        contract::init(&env, &admin, &factory, &config)
    }

    /// Registers a new member in the circle.
    pub fn join(env: Env, member: Address) -> ContractResult {
        contract::join(&env, &member)
    }

    /// Invites multiple candidate addresses in a single call.
    pub fn batch_invite(env: Env, caller: Address, members: AddressList) -> ContractResult {
        contract::batch_invite(&env, &caller, &members)
    }

    /// Exits a member from the circle according to circle exit rules.
    pub fn exit_circle(env: Env, member: Address) -> ContractResult {
        contract::exit(&env, &member)
    }

    /// Cancels the circle.
    pub fn cancel_circle(env: Env, caller: Address) -> ContractResult {
        contract::cancel_circle(&env, &caller)
    }

    /// Cancels an active round or circle operation.
    pub fn cancel(env: Env, caller: Address) -> ContractResult {
        contract::cancel(&env, &caller)
    }

    /// Pauses circle operations.
    pub fn pause_circle(env: Env, admin: Address) -> ContractResult {
        contract::pause_circle(&env, &admin)
    }

    /// Resumes paused circle operations.
    pub fn unpause_circle(env: Env, admin: Address) -> ContractResult {
        contract::unpause_circle(&env, &admin)
    }

    // =========================================================================
    // Contributions, Payouts & Auctions
    // =========================================================================

    /// Records a member contribution of `amount` for the given `round`.
    pub fn contribute(env: Env, member: Address, amount: i128, round: u32) -> ContractResult {
        contract::contribute(&env, &member, amount, round)
    }

    /// Enforces or checks the contribution deadline for the active round.
    pub fn check_contribution_deadline(env: Env) -> ContractResult {
        contract::check_contribution_deadline(&env)
    }

    /// Executes the payout for `round` when conditions are met.
    pub fn trigger_payout(env: Env, caller: Address, round: u32) -> ContractResult {
        contract::trigger_payout(&env, &caller, round)
    }

    /// Submits an auction bid with `discount_bips` for `round`.
    pub fn auction_bid(
        env: Env,
        bidder: Address,
        discount_bips: u32,
        round: u32,
    ) -> ContractResult {
        contract::auction_bid(&env, &bidder, discount_bips, round)
    }

    /// Refunds a capped batch of losing auction deposits. Returns how many
    /// losers remain; call again until the result is 0 (#436).
    pub fn refund_losing_bids(
        env: Env,
        caller: Address,
        round: u32,
        limit: u32,
    ) -> ContractResult<u32> {
        contract::refund_losing_bids(&env, &caller, round, limit)
    }

    /// Initializes a Dutch auction for `round` with starting/floor basis points,
    /// per-ledger decay, and expiration window.
    pub fn init_dutch_auction(
        env: Env,
        caller: Address,
        round: u32,
        start_bips: u32,
        floor_bips: u32,
        decay_bips_per_ledger: u32,
        expiry_ledgers: u32,
    ) -> ContractResult {
        contract::init_dutch_auction(
            &env,
            &caller,
            round,
            start_bips,
            floor_bips,
            decay_bips_per_ledger,
            expiry_ledgers,
        )
    }

    /// Returns the current Dutch auction price in basis points and whether it is active.
    pub fn dutch_auction_price(env: Env, round: u32) -> ContractResult<(u32, bool)> {
        contract::dutch_auction_price(&env, round)
    }

    /// Places a Dutch auction bid for `round` and returns the settled price in basis points.
    pub fn dutch_auction_bid(env: Env, bidder: Address, round: u32) -> ContractResult<u32> {
        contract::dutch_auction_bid(&env, &bidder, round)
    }

    /// Casts a member vote for `vote_for` to receive the payout in `round`.
    pub fn vote_payout(
        env: Env,
        voter: Address,
        vote_for: Address,
        round: u32,
    ) -> ContractResult {
        contract::vote_payout(&env, &voter, &vote_for, round)
    }

    /// #470 — Cancel an unresolved auction and refund the highest bid atomically.
    pub fn cancel_auction(env: Env, caller: Address) -> ContractResult {
        contract::cancel_auction(&env, &caller)
    }

    /// Schedules a payout for `round`.
    pub fn schedule_payout(env: Env, caller: Address, round: u32) -> ContractResult {
        contract::schedule_payout(&env, &caller, round)
    }

    /// Returns whether a payout is scheduled for `round`.
    pub fn is_payout_scheduled(env: Env, round: u32) -> bool {
        contract::is_payout_scheduled(&env, round)
    }

    /// Executes a batch payout to `recipients` with corresponding `amounts` for `round`.
    pub fn batch_payout(
        env: Env,
        caller: Address,
        recipients: AddressList,
        amounts: SorobanVec<i128>,
        round: u32,
    ) -> ContractResult {
        contract::batch_payout(&env, &caller, &recipients, &amounts, round)
    }

    // =========================================================================
    // Late Reporting & Disputes
    // =========================================================================

    /// Reports `late_member` for missing a contribution deadline in `round`.
    pub fn report_late(
        env: Env,
        reporter: Address,
        late_member: Address,
        round: u32,
    ) -> ContractResult {
        contract::report_late(&env, &reporter, &late_member, round)
    }

    /// Submits a dispute with an off-chain `evidence_hash`.
    pub fn dispute(env: Env, member: Address, evidence_hash: BytesN<32>) -> ContractResult {
        contract::dispute(&env, &member, &evidence_hash)
    }

    /// Raises a formal dispute with an off-chain `evidence_hash`.
    pub fn raise_dispute(env: Env, member: Address, evidence_hash: BytesN<32>) -> ContractResult {
        contract::raise_dispute(&env, &member, &evidence_hash)
    }

    /// Resolves an open dispute with `resolution` code.
    pub fn resolve_dispute(env: Env, admin: Address, resolution: u32) -> ContractResult {
        contract::resolve_dispute(&env, &admin, resolution)
    }

    /// Returns the recorded dispute resolution, if one exists.
    pub fn get_dispute_resolution(env: Env) -> Option<DisputeResolutionRecord> {
        contract::get_dispute_resolution(&env)
    }

    // =========================================================================
    // Referrals, Streaks & Treasury
    // =========================================================================

    /// Registers a referral relationship with a percentage bonus (`bonus_pct`).
    pub fn register_referral(
        env: Env,
        referrer: Address,
        referred: Address,
        bonus_pct: u32,
    ) -> ContractResult {
        contract::register_referral(&env, &referrer, &referred, bonus_pct)
    }

    /// Claims accrued referral rewards for `referrer`.
    pub fn claim_referral_bonus(env: Env, referrer: Address) -> ContractResult {
        contract::claim_referral_bonus(&env, &referrer)
    }

    /// Updates on-time contribution streak tracking for `member` in `round`.
    pub fn update_streak(env: Env, member: Address, round: u32) -> ContractResult {
        contract::update_streak(&env, &member, round)
    }

    /// Claims earned streak rewards for `member`.
    pub fn claim_streak_bonus(env: Env, member: Address) -> ContractResult {
        contract::claim_streak_bonus(&env, &member)
    }
    /// #478 — Organizer/admin treasury withdrawal, capped per transaction and per UTC day.
    pub fn withdraw_treasury(env: Env, caller: Address, amount: i128) -> ContractResult {
        contract::withdraw_treasury(&env, &caller, amount)
    }

    /// #357 — update mutable circle metadata (name, slug).
    pub fn update_metadata(
        env: Env,
        caller: Address,
        field: soroban_sdk::String,
        value: soroban_sdk::String,
    ) -> ContractResult {
        contract::update_metadata(&env, &caller, field, value)
    }

    // =========================================================================
    // Configuration & Admin
    // =========================================================================

    /// Applies initial configuration values passed from the factory contract.
    pub fn configure_from_factory(
        env: Env,
        factory: Address,
        treasury: Address,
        reputation_registry: Address,
        fee_bps: u32,
    ) -> ContractResult {
        contract::configure_from_factory(&env, &factory, &treasury, &reputation_registry, fee_bps)
    }

    /// Sets the external reputation registry address.
    pub fn set_reputation_registry(
        env: Env,
        admin: Address,
        registry: Address,
    ) -> ContractResult {
        contract::set_reputation_registry(&env, &admin, &registry)
    }

    /// Sets the treasury address for protocol/circle funds.
    pub fn set_treasury(env: Env, admin: Address, treasury: Address) -> ContractResult {
        contract::set_treasury(&env, &admin, &treasury)
    }

    /// Sets the circle contribution/payout token contract address.
    pub fn set_token(env: Env, admin: Address, token: Address) -> ContractResult {
        contract::set_token(&env, &admin, &token)
    }

    /// Updates the circle fee in basis points (`fee_bps`).
    pub fn set_fee_bps(env: Env, admin: Address, fee_bps: u32) -> ContractResult {
        contract::set_fee_bps(&env, &admin, fee_bps)
    }

    /// Replaces the member allowlist for circles restricting entry.
    pub fn set_allowlist(env: Env, admin: Address, allowlist: AddressList) -> ContractResult {
        contract::set_allowlist(&env, &admin, allowlist)
    }

    /// Sets the primary oracle contract address.
    pub fn set_oracle(env: Env, admin: Address, oracle: Address) -> ContractResult {
        contract::set_oracle(&env, &admin, &oracle)
    }

    /// Sets the fallback oracle contract address.
    pub fn set_fallback_oracle(env: Env, admin: Address, oracle: Address) -> ContractResult {
        contract::set_fallback_oracle(&env, &admin, &oracle)
    }

    // =========================================================================
    // Read / Query Methods
    // =========================================================================

    /// Returns the current circle state.
    pub fn get_status(env: Env) -> CircleState {
        contract::get_status(&env)
    }

    /// #469 — Top contributors by total received, newest-round weighted.
    pub fn query_top_contributors(env: Env, n: u32) -> ContractResult<TopContributorsList> {
        contract::query_top_contributors(&env, n)
    }

    /// Returns all members currently in the circle.
    pub fn get_members(env: Env) -> MemberList {
        contract::get_members(&env)
    }

    /// Returns paginated contribution records for `member`.
    pub fn get_contributions(
        env: Env,
        member: Address,
        page: u32,
        page_size: u32,
    ) -> ContributionList {
        contract::get_contributions(&env, &member, page, page_size)
    }

    /// Returns the stored configuration hash for `round`.
    pub fn query_round_config(env: Env, round: u32) -> ContractResult<BytesN<32>> {
        contract::query_round_config(&env, round)
    }

    /// Returns the round fee ledger record for `round`.
    pub fn get_round_fee_ledger(env: Env, round: u32) -> RoundFeeLedger {
        contract::get_round_fee_ledger(&env, round)
    }

    /// Returns all registered referrals.
    pub fn get_referrals(env: Env) -> ReferralList {
        contract::get_referrals(&env)
    }

    /// Returns all member streak records.
    pub fn get_streaks(env: Env) -> StreakList {
        contract::get_streaks(&env)
    }

    /// Returns the streak record for a specific `member`.
    pub fn get_member_streak(env: Env, member: Address) -> Streak {
        contract::get_member_streak(&env, &member)
    }

    /// Returns the configured reputation registry address, if set.
    pub fn get_reputation_registry(env: Env) -> Option<Address> {
        contract::get_reputation_registry(&env)
    }

    /// Returns the configured member allowlist.
    pub fn get_allowlist(env: Env) -> AddressList {
        contract::get_allowlist(&env)
    }

    /// Returns the primary oracle address, if configured.
    pub fn get_oracle(env: Env) -> Option<Address> {
        contract::get_oracle(&env)
    }

    /// Returns the fallback oracle address, if configured.
    pub fn get_fallback_oracle(env: Env) -> Option<Address> {
        contract::get_fallback_oracle(&env)
    }

    /// Returns the configured treasury address, if set.
    pub fn get_treasury(env: Env) -> Option<Address> {
        contract::get_treasury(&env)
    }

    /// Returns the configured fee in basis points.
    pub fn get_fee_bps(env: Env) -> u32 {
        contract::get_fee_bps(&env)
    }

    /// #357 — Returns the circle description metadata, if set.
    pub fn get_description(env: Env) -> Option<soroban_sdk::String> {
        contract::get_description(&env)
    }
}