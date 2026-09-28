package apperrors

import "errors"

// Circle domain errors mapped from CircleError contract error variants.
var (
	ErrCircleNotInitialized        = errors.New("circle not initialized")
	ErrCircleNotActive             = errors.New("circle not active")
	ErrCircleFull                  = errors.New("circle is full")
	ErrAlreadyMember               = errors.New("address is already a member")
	ErrNotMember                   = errors.New("address is not a member")
	ErrMoiScoreTooLow              = errors.New("moi score insufficient")
	ErrRoundNotCurrent             = errors.New("round is not current")
	ErrInvalidAmount               = errors.New("invalid amount")
	ErrPaymentDeadlinePassed       = errors.New("payment deadline passed")
	ErrMaxStrikesReached           = errors.New("maximum strikes reached")
	ErrNotOrganizer                = errors.New("caller is not the circle organizer")
	ErrContractPaused              = errors.New("contract is paused")
	ErrInvalidInviteCode           = errors.New("invalid invite code")
	ErrAuctionAlreadyResolved      = errors.New("auction already resolved")
	ErrVoteQuorumNotMet            = errors.New("vote quorum not met")
	ErrAlreadyContributed          = errors.New("member already contributed for this round")
	ErrAlreadyVoted                = errors.New("member already voted for this round")
	ErrAlreadyBidded               = errors.New("member already placed bid for this round")
	ErrPayoutAlreadyExecuted       = errors.New("payout already executed")
	ErrInvalidPayoutType           = errors.New("invalid payout type")
	ErrInvalidRound                = errors.New("invalid round")
	ErrContributionMismatch        = errors.New("contribution amount mismatch")
	ErrCircleNotFull               = errors.New("circle is not full")
	ErrNotEnoughVotes              = errors.New("not enough votes")
	ErrDisputeAlreadyRaised        = errors.New("dispute already raised")
	ErrNoActiveDispute             = errors.New("no active dispute")
	ErrUnauthorized                = errors.New("unauthorized")
	ErrInvalidBid                  = errors.New("invalid bid")
	ErrInvalidMemberStatus         = errors.New("invalid member status")
	ErrEmptyPayoutOrder            = errors.New("empty payout order")
	ErrCircleSizeExceedsTier       = errors.New("circle size exceeds organizer tier limit")
	ErrContributionExceedsTier     = errors.New("contribution amount exceeds organizer tier limit")
	ErrVecAccessError              = errors.New("vector access error")
	ErrAllowlistNotPermitted       = errors.New("member not on allowlist")
	ErrInsufficientContractBalance = errors.New("insufficient contract balance")
	ErrSelfReferral                = errors.New("self referral not permitted")
	ErrOracleUnavailable          = errors.New("oracle unavailable")
	ErrPayoutAlreadyScheduled      = errors.New("payout already scheduled")
	ErrDutchAuctionNotConfigured   = errors.New("dutch auction not configured")
	ErrDutchAuctionExpired         = errors.New("dutch auction expired")
	ErrInvalidDutchConfig          = errors.New("invalid dutch auction configuration")
	ErrAuctionNotResolved          = errors.New("auction not resolved")
)

// Factory domain errors mapped from FactoryError contract error variants.
var (
	ErrFactoryNotInitialized = errors.New("factory not initialized")
	ErrWasmHashNotSet        = errors.New("wasm hash not set")
	ErrInvalidFeeBps         = errors.New("invalid fee bps")
	ErrCircleDeployFailed    = errors.New("circle deployment failed")
	ErrInvalidConfig         = errors.New("invalid configuration")
	ErrRateLimitExceeded     = errors.New("rate limit exceeded")
)

// Treasury domain errors mapped from TreasuryError contract error variants.
var (
	ErrTreasuryNotInitialized = errors.New("treasury not initialized")
	ErrInsufficientBalance    = errors.New("insufficient treasury balance")
	ErrAlreadyInitialized     = errors.New("already initialized")
	ErrContractNotPaused      = errors.New("contract is not paused")
)

// Reputation domain errors mapped from ReputationError contract error variants.
var (
	ErrInvalidActivityType = errors.New("invalid activity type")
	ErrScoreNotFound       = errors.New("score not found")
	ErrInvalidScoreImpact  = errors.New("invalid score impact")
	ErrScoreOverflow       = errors.New("reputation score overflow")
)

// Staking domain errors mapped from StakingError contract error variants.
var (
	ErrAlreadyStaked       = errors.New("already staked")
	ErrNoActiveStake       = errors.New("no active stake")
	ErrInvalidPeriod       = errors.New("invalid staking period")
	ErrLockPeriodNotEnded  = errors.New("lock period has not ended")
	ErrNoUnbondingPosition = errors.New("no unbonding position")
	ErrUnbondingNotEnded   = errors.New("unbonding period has not ended")
	ErrAmountBelowMinimum  = errors.New("amount below minimum threshold")
)

// Governance domain errors mapped from GovernanceError contract error variants.
var (
	ErrProposalNotFound  = errors.New("proposal not found")
	ErrProposalNotActive = errors.New("proposal is not active")
	ErrAlreadyVotedGov   = errors.New("already voted on proposal")
	ErrVotingClosed      = errors.New("voting period is closed")
	ErrTimelockNotMet    = errors.New("timelock not met")
	ErrProposalPassed    = errors.New("proposal already passed")
	ErrProposalFailed    = errors.New("proposal failed")
)
