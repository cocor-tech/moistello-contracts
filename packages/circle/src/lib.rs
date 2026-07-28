#![cfg_attr(not(test), no_std)]
mod types; mod contract; mod payout; #[cfg(test)] mod test;
use soroban_sdk::{contract,contractimpl,Address,BytesN,Env};
#[contract]pub struct Circle;
#[contractimpl]impl Circle{
    pub fn __constructor(env:Env,admin:Address,factory:Address,config:types::CircleConfig)->Result<(),types::CircleError>{contract::init(&env,&admin,&factory,&config)}
    pub fn join(env:Env,member:Address)->Result<(),types::CircleError>{contract::join(&env,&member)}
    pub fn contribute(env:Env,member:Address,amount:i128,round:u32)->Result<(),types::CircleError>{contract::contribute(&env,&member,amount,round)}
    pub fn trigger_payout(env:Env,caller:Address,round:u32)->Result<(),types::CircleError>{contract::trigger_payout(&env,&caller,round)}
    pub fn auction_bid(env:Env,bidder:Address,discount_bips:u32,round:u32)->Result<(),types::CircleError>{contract::auction_bid(&env,&bidder,discount_bips,round)}
    pub fn vote_payout(env:Env,voter:Address,vote_for:Address,round:u32)->Result<(),types::CircleError>{contract::vote_payout(&env,&voter,&vote_for,round)}
    pub fn exit_circle(env:Env,member:Address)->Result<(),types::CircleError>{contract::exit(&env,&member)}
    pub fn report_late(env:Env,reporter:Address,late_member:Address,round:u32)->Result<(),types::CircleError>{contract::report_late(&env,&reporter,&late_member,round)}
    pub fn raise_dispute(env:Env,member:Address,evidence_hash:BytesN<32>)->Result<(),types::CircleError>{contract::raise_dispute(&env,&member,&evidence_hash)}
    pub fn resolve_dispute(env:Env,admin:Address,resolution:u32)->Result<(),types::CircleError>{contract::resolve_dispute(&env,&admin,resolution)}
    pub fn get_status(env:Env)->types::Circle{contract::get_status(&env)}
    pub fn get_members(env:Env)->soroban_sdk::Vec<types::Member>{contract::get_members(&env)}
    pub fn get_contributions(env:Env,member:Address)->soroban_sdk::Vec<types::Contribution>{contract::get_contributions(&env,&member)}
    pub fn pause_circle(env:Env,admin:Address)->Result<(),types::CircleError>{contract::pause_circle(&env,&admin)}
    pub fn unpause_circle(env:Env,admin:Address)->Result<(),types::CircleError>{contract::unpause_circle(&env,&admin)}
}
