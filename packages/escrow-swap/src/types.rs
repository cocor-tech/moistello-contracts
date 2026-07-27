use soroban_sdk::{contracttype,contracterror,contractevent,Address,BytesN};
#[contracttype]#[derive(Clone,Debug)]
pub struct SwapRequest{pub id:u64,pub initiator:Address,pub responder:Address,pub initiator_amount:i128,pub responder_amount:i128,pub hash_lock:BytesN<32>,pub time_lock:u64,pub status:u32,pub created_at:u64}
pub const STATUS_PENDING:u32=0;pub const STATUS_ACTIVE:u32=1;pub const STATUS_COMPLETED:u32=2;pub const STATUS_CANCELLED:u32=3;
#[contracttype]#[derive(Clone)]
pub enum DataKey{Admin,SwapRequests,NextSwapId,Paused}
#[contracterror]#[derive(Debug,Clone,PartialEq,Eq)]
pub enum EscrowError{NotInitialized=1,Unauthorized=2,ContractPaused=3,InvalidAmount=4,SwapNotFound=5,SwapNotActive=6,AlreadyAccepted=7,HashLockMismatch=8,TimeLockExpired=9,InsufficientBalance=10,InvalidSwap=11,VecAccessError=12}
#[contractevent]#[derive(Clone,Debug)]
pub struct SwapCreated{pub id:u64,pub initiator:Address,pub responder:Address,pub initiator_amount:i128,pub responder_amount:i128}
#[contractevent]#[derive(Clone,Debug)]
pub struct SwapAccepted{pub id:u64,pub responder:Address}
#[contractevent]#[derive(Clone,Debug)]
pub struct SwapCompleted{pub id:u64,pub initiator:Address,pub responder:Address}
#[contractevent]#[derive(Clone,Debug)]
pub struct SwapCancelled{pub id:u64}