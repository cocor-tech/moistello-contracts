use soroban_sdk::{Address,Env,Vec,Map};
use crate::types::*;
use crate::voting;
use common::vrf;

pub fn resolve_random(env:&Env,circle:&Circle,round:u32)->Result<Address,CircleError>{
    let positions=vrf::shuffle_positions(env,circle.max_members, round * 1000).map_err(|_| CircleError::InvalidAmount)?;
    let members:Vec<Member>=env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut pos_to_addr:Map<u32,Address>=Map::new(env);
    for i in 0..members.len(){
        let m=members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status==MEMBER_ACTIVE{
            pos_to_addr.set(m.position,m.address.clone());
        }
    }
    for i in 0..positions.len(){
        let pos=positions.get(i).ok_or(CircleError::NotInitialized)?;
        if(circle.payout_bitmap&(1u128<<pos))==0{
            if let Some(addr)=pos_to_addr.get(pos){
                return Ok(addr);
            }
        }
    }
    for i in 0..members.len() {
        let m = members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status == MEMBER_ACTIVE && (circle.payout_bitmap & (1u128 << m.position)) == 0 {
            return Ok(m.address);
        }
    }
    Err(CircleError::PayoutAlreadyExecuted)
}

pub fn resolve_fixed(env:&Env,circle:&Circle,round:u32)->Result<Address,CircleError>{
    let pos=(round%circle.max_members)as u32;
    if(circle.payout_bitmap&(1u128<<pos))!=0{
        return Err(CircleError::PayoutAlreadyExecuted);
    }
    let members:Vec<Member>=env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut pos_to_addr:Map<u32,Address>=Map::new(env);
    for i in 0..members.len(){
        let m=members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.status==MEMBER_ACTIVE{
            pos_to_addr.set(m.position,m.address.clone());
        }
    }
    pos_to_addr.get(pos).ok_or(CircleError::NotMember)
}

pub fn resolve_auction(env:&Env,circle:&Circle,round:u32)->Result<(Address,u32),CircleError>{
    let bids:Vec<AuctionBid>=env.storage().persistent().get(&DataKey::Bids).unwrap_or_else(||Vec::new(env));
    let members:Vec<Member>=env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut max_bps:u32=0;
    let mut winner:Option<AuctionBid>=None;
    for i in 0..bids.len(){
        let b=bids.get(i).ok_or(CircleError::NotInitialized)?;
        if b.round==round&&b.discount_bips>=max_bps{max_bps=b.discount_bips;winner=Some(b);}
    }
    let winner_bid=winner.ok_or(CircleError::VoteQuorumNotMet)?;
    for i in 0..members.len(){
        let m=members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address==winner_bid.bidder{
            if(circle.payout_bitmap&(1u128<<m.position))!=0{
                return Err(CircleError::PayoutAlreadyExecuted);
            }
            return Ok((winner_bid.bidder,winner_bid.discount_bips));
        }
    }
    Err(CircleError::NotMember)
}

/// Resolves a `PAYOUT_VOTE` round with quadratic, weighted voting (#330).
///
/// Each member still casts at most one vote per round (`vote_payout` rejects a
/// repeat with `AlreadyVoted`), but each vote now carries a quadratic weight
/// derived from the voter's balance of the circle token:
///
/// ```text
/// weight = 1 + min(isqrt(balance / 10^7), 1000)   // see crate::voting
/// ```
///
/// The tally therefore accumulates *weight* per nominee instead of raw votes,
/// which makes large-scale vote buying unprofitable (a 100x balance advantage
/// only buys 10x the influence) while every member keeps a base vote.
///
/// Quorum is checked on the same weighted scale: the weight cast in the round
/// must reach `floor(active_members / 2) + 1`. For equal-weight voters this is
/// identical to the previous head-count quorum, so existing behaviour is
/// preserved; heavier voters simply reach the bar with fewer participants.
pub fn resolve_vote(env:&Env,circle:&Circle,round:u32)->Result<Address,CircleError>{
    let votes:Vec<VoteEntry>=env.storage().persistent().get(&DataKey::Votes).unwrap_or_else(||Vec::new(env));
    let members:Vec<Member>=env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let active=count_active(env)?;
    let mut tally:Map<Address,u32>=Map::new(env);
    let mut cast_weight:u128=0;
    for i in 0..votes.len(){
        let v=votes.get(i).ok_or(CircleError::VecAccessError)?;
        if v.round==round{
            let w=voting::vote_weight(env,circle,&v.voter);
            cast_weight=cast_weight.saturating_add(u128::from(w));
            let c=tally.get(v.vote_for.clone()).unwrap_or(0);
            tally.set(v.vote_for.clone(),c.saturating_add(w));
        }
    }
    voting::check_quorum(cast_weight,active)?;
    let mut best_addr:Option<Address>=None;
    let mut best_count:u32=0;
    for(addr,count)in tally.iter(){
        if count>best_count{best_count=count;best_addr=Some(addr);}
    }
    let winner=best_addr.ok_or(CircleError::VoteQuorumNotMet)?;
    for i in 0..members.len(){
        let m=members.get(i).ok_or(CircleError::VecAccessError)?;
        if m.address==winner{
            if(circle.payout_bitmap&(1u128<<m.position))!=0{
                return Err(CircleError::PayoutAlreadyExecuted);
            }
            return Ok(winner);
        }
    }
    Err(CircleError::NotMember)
}

fn count_active(env:&Env)->Result<u32,CircleError>{
    let members:Vec<Member>=env.storage().persistent().get(&DataKey::Members).ok_or(CircleError::NotInitialized)?;
    let mut c:u32=0;
    for i in 0..members.len(){
        if members.get(i).ok_or(CircleError::NotInitialized)?.status==MEMBER_ACTIVE{
            c=c.checked_add(1).ok_or(CircleError::InvalidAmount)?;
        }
    }
    Ok(c)
}

