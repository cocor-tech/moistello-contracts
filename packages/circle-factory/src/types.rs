use soroban_sdk::{contracttype, contracterror, contractevent, Address, String, Vec};
pub use common::types::CircleConfig;
#[contracttype]#[derive(Clone,Debug)]pub struct FeeConfig{pub fee_bps:i128,pub updated_at:u64,pub updated_by:Address}
#[contracttype]#[derive(Clone,Debug)]pub struct CircleEntry{pub circle_id:Address,pub name:String,pub organizer:Address,pub deployed_at:u64,pub status:u32}
#[contracttype]#[derive(Clone,Debug)]pub struct CircleRegistry{pub circles:Vec<CircleEntry>}
#[contracttype]#[derive(Clone)]pub enum DataKey{Admin,FeeConfig,CircleList,CircleCount,WasmHash}
#[contracterror]#[derive(Debug,Clone,PartialEq,Eq)]pub enum FactoryError{NotInitialized=1,Unauthorized=2,ContractPaused=3,WasmHashNotSet=4,InvalidFeeBps=5,CircleDeployFailed=6,InvalidConfig=7,AlreadyInitialized=8,InvalidAdmin=9}
#[contractevent(topics=["deployed"])]#[derive(Clone,Debug)]pub struct CircleDeployed{#[topic]pub creator:Address,#[topic]pub circle_id:Address,pub name:String}
#[contractevent(topics=["fee_cfg"])]#[derive(Clone,Debug)]pub struct FeeConfigUpdated{#[topic]pub updated_by:Address,pub old_fee_bps:i128,pub new_fee_bps:i128}
