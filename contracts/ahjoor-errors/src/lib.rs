//! # ahjoor-errors — Global Error Code Namespace Registry
//!
//! Each Ahjoor protocol contract owns a non-overlapping numeric range so that
//! off-chain parsers can unambiguously decode `InvokeHostFunctionTrapped` errors
//! without per-contract decode tables.
//!
//! ## Range allocation
//!
//! | Contract              | Range       |
//! |----------------------|-------------|
//! | ahjoor-rosca         | 1000 – 1299 |
//! | ahjoor-payments      | 2000 – 2299 |
//! | ahjoor-escrow        | 3000 – 3299 |
//! | ahjoor-refund        | 4000 – 4099 |
//! | ahjoor-token-whitelist | 5000 – 5099 |
//!
//! On-chain contracts continue to use their existing small discriminants (1–118
//! for rosca, 1–56 for payments, etc.) because `#[contracterror]` must produce
//! values that fit in the Soroban XDR `ScError` u32 field and the existing enum
//! variants are already deployed.  This crate provides the *off-chain* namespace
//! that relay nodes and indexers use when decoding errors across contracts.

// ---------------------------------------------------------------------------
// ahjoor-rosca (1000–1299)
// ---------------------------------------------------------------------------

pub mod rosca {
    // Core Error variants (on-chain discriminant → namespaced code)
    pub const ALREADY_INITIALIZED: u32         = 1001;
    pub const TOKEN_NOT_APPROVED: u32          = 1002;
    pub const CUSTOM_ORDER_LENGTH_MISMATCH: u32 = 1003;
    pub const CUSTOM_ORDER_NON_MEMBER: u32     = 1004;
    pub const AMOUNT_MUST_BE_POSITIVE: u32     = 1005;
    pub const ROUND_DEADLINE_PASSED: u32       = 1006;
    pub const MEMBER_HAS_EXITED: u32           = 1007;
    pub const NOT_A_MEMBER: u32                = 1008;
    pub const ALREADY_CONTRIBUTED: u32         = 1009;
    pub const INVALID_EXCHANGE_RATE: u32       = 1010;
    pub const EXCEEDS_TOKEN_LIMIT: u32         = 1011;
    pub const EXCEEDS_REMAINING_CONTRIBUTION: u32 = 1012;
    pub const DEADLINE_NOT_PASSED: u32         = 1013;
    pub const PENALTY_DISABLED: u32            = 1014;
    pub const NOT_A_DEFAULTER: u32             = 1015;
    pub const CANNOT_CHANGE_MID_ROUND: u32     = 1016;
    pub const ALREADY_A_MEMBER: u32            = 1017;
    pub const NO_REWARDS_TO_CLAIM: u32         = 1018;
    pub const ONLY_MEMBERS_ALLOWED: u32        = 1019;
    pub const PROPOSAL_NOT_FOUND: u32          = 1020;
    pub const VOTING_DEADLINE_PASSED: u32      = 1021;
    pub const PROPOSAL_NOT_PENDING: u32        = 1022;
    pub const ALREADY_VOTED: u32               = 1023;
    pub const VOTING_NOT_ENDED: u32            = 1024;
    pub const CONTRACT_PAUSED: u32             = 1025;
    pub const ALL_MEMBERS_SUSPENDED: u32       = 1026;
    pub const ALREADY_PAUSED: u32             = 1027;
    pub const NOT_PAUSED: u32                  = 1028;
    pub const MEMBER_ALREADY_EXITED: u32       = 1029;
    pub const EXIT_REQUEST_PENDING: u32        = 1030;
    pub const NO_EXIT_REQUEST_FOUND: u32       = 1031;
    pub const EXIT_NOT_ALLOWED_MID_ROUND: u32  = 1032;
    pub const CONTRIBUTION_WINDOW_CLOSED: u32  = 1033;
    pub const FEE_EXCEEDS_MAXIMUM: u32         = 1034;
    pub const INVALID_MAX_DEFAULTS: u32        = 1035;
    pub const GROUP_FULL: u32                  = 1036;
    pub const INVALID_MAX_MEMBERS: u32         = 1037;
    pub const DELEGATION_ALREADY_EXISTS: u32   = 1038;
    pub const NO_DELEGATION_FOUND: u32         = 1039;
    pub const CANNOT_VOTE_WITH_ACTIVE_DELEGATION: u32 = 1040;
    pub const CANNOT_SUB_DELEGATE: u32         = 1041;
    pub const INVITE_NOT_FOUND: u32            = 1042;
    pub const INVITE_ALREADY_REDEEMED: u32     = 1043;
    pub const INVITE_WRONG_RECIPIENT: u32      = 1044;
    pub const ADMIN_ACTION_NOT_FOUND: u32      = 1045;
    pub const ADMIN_ACTION_ALREADY_EXECUTED: u32 = 1046;
    pub const ADMIN_ACTION_EXPIRED: u32        = 1047;
    pub const ADMIN_ALREADY_APPROVED: u32      = 1048;
    pub const INSUFFICIENT_APPROVALS: u32      = 1049;
    pub const NOT_A_CO_ADMIN: u32             = 1050;
    // ExtError variants
    pub const INVALID_TIER: u32               = 1051;
    pub const INSURANCE_POOL_NEGATIVE: u32    = 1052;
    pub const INVALID_INSURANCE_CONTRIBUTION: u32 = 1053;
    pub const SKIP_LIMIT_REACHED: u32         = 1054;
    pub const ALREADY_SKIPPED: u32            = 1055;
    pub const INSUFFICIENT_WEIGHT: u32        = 1056;
    pub const EMERGENCY_PAYOUT_REQUESTED: u32 = 1057;
    pub const EMERGENCY_PAYOUT_QUORUM_NOT_MET: u32 = 1058;
    pub const EMERGENCY_PAYOUT_VOTE_EXPIRED: u32 = 1059;
    pub const EMERGENCY_PAYOUT_ALREADY_EXECUTED: u32 = 1060;
    pub const EMERGENCY_PAYOUT_LIMIT_REACHED: u32 = 1061;
    pub const GROUP_ALREADY_DISSOLVED: u32    = 1062;
    pub const DISSOLUTION_VOTE_IN_PROGRESS: u32 = 1063;
    pub const DISSOLUTION_QUORUM_NOT_MET: u32 = 1064;
    pub const DISSOLUTION_VOTE_EXPIRED: u32   = 1065;
    pub const NO_FUNDS_TO_DISTRIBUTE: u32     = 1066;
    pub const INVALID_EMERGENCY_CONFIG: u32   = 1067;
    pub const INVALID_DISSOLUTION_CONFIG: u32 = 1068;
    pub const GROUP_NOT_YET_ACTIVE: u32       = 1069;
    pub const ONLY_ADMIN_ALLOWED: u32         = 1070;
    pub const INVALID_AMOUNT: u32             = 1071;
    pub const CO_SIGNER_ALREADY_SET: u32      = 1072;
    pub const NO_CO_SIGNER_FOUND: u32         = 1073;
    pub const CO_SIGNER_NOT_ACCEPTED: u32     = 1074;
    pub const NOT_THE_CO_SIGNER: u32          = 1075;
    pub const CO_SIGNER_WINDOW_NOT_OPEN: u32  = 1076;
    pub const CO_SIGNER_WINDOW_EXPIRED: u32   = 1077;
    pub const GROUP_FROZEN: u32               = 1078;
    pub const GROUP_NOT_FROZEN: u32           = 1079;
    pub const SNAPSHOT_TOO_SOON: u32          = 1080;
    pub const TIER_NOT_FOUND: u32             = 1081;
    pub const INVALID_TIER_DEFINITION: u32    = 1082;
    pub const INSUFFICIENT_CREDIT_SCORE: u32  = 1083;
    pub const ROUND_DURATION_OUT_OF_BOUNDS: u32 = 1084;
    pub const DELEGATION_EXPIRED: u32         = 1085;
    pub const NOT_CONTRIB_DELEGATE: u32       = 1086;
    pub const SPLIT_PROPOSAL_NOT_FOUND: u32   = 1087;
    pub const SPLIT_MEMBERS_INVALID: u32      = 1088;
    pub const SPLIT_CONFIRMATION_WINDOW_CLOSED: u32 = 1089;
    pub const SOURCE_GROUP_ALREADY_SPLIT: u32 = 1090;
    pub const SPLIT_ALREADY_CONFIRMED: u32    = 1091;
    pub const SPLIT_NOT_FULLY_CONFIRMED: u32  = 1092;
    // ExtError2 variants
    pub const AUCTION_NOT_ENABLED: u32        = 1101;
    pub const AUCTION_NOT_OPEN: u32           = 1102;
    pub const AUCTION_WINDOW_CLOSED: u32      = 1103;
    pub const INCORRECT_CONTRIBUTION_AMOUNT: u32 = 1104;
    pub const INVALID_SLOT_INDEX: u32         = 1105;
    pub const MIGRATION_ALREADY_EXECUTED: u32 = 1106;
    pub const MIGRATION_ALREADY_PENDING: u32  = 1107;
    pub const MIGRATION_NOT_APPROVED: u32     = 1108;
    pub const MIGRATION_NOT_FOUND: u32        = 1109;
    pub const NO_BID_FOUND: u32              = 1110;
    pub const SLOT_OCCUPIED: u32              = 1111;
    pub const TOKEN_MISMATCH: u32             = 1112;
    pub const OUTSTANDING_LOAN_EXISTS: u32    = 1113;
    pub const NO_COPAYERS_REGISTERED: u32     = 1114;
    pub const COPAYER_AMOUNTS_MISMATCH: u32   = 1115;
    pub const RECEIPT_NOT_FOUND: u32          = 1116;
    pub const COPAYER_SPLITS_ALREADY_SET: u32 = 1117;
    pub const PROXY_ROUNDS_EXHAUSTED: u32     = 1118;
}

// ---------------------------------------------------------------------------
// ahjoor-payments (2000–2299)
// ---------------------------------------------------------------------------

pub mod payments {
    pub const RATE_LIMIT_EXCEEDED: u32              = 2001;
    pub const SUBSCRIPTION_PAUSED: u32              = 2002;
    pub const ORACLE_CONDITION_NOT_MET: u32         = 2003;
    pub const SUBSCRIPTION_IN_TRIAL: u32            = 2004;
    pub const TOKEN_NOT_ALLOWED: u32                = 2005;
    pub const DUPLICATE_EXTERNAL_ID: u32            = 2006;
    pub const MULTISIG_NOT_REQUIRED: u32            = 2007;
    pub const ALREADY_APPROVED: u32                 = 2008;
    pub const NOT_A_SIGNER: u32                     = 2009;
    pub const VOUCHER_EXPIRED: u32                  = 2010;
    pub const VOUCHER_EXHAUSTED: u32                = 2011;
    pub const VOUCHER_REVOKED: u32                  = 2012;
    pub const VOUCHER_NOT_FOUND: u32                = 2013;
    pub const WITHDRAWAL_RATE_LIMIT_EXCEEDED: u32   = 2014;
    pub const REFERRAL_ALREADY_EXISTS: u32          = 2015;
    pub const NO_COMMISSION_TO_CLAIM: u32           = 2016;
    pub const DYNAMIC_PAYMENT_EXPIRED: u32          = 2017;
    pub const TIPPING_NOT_ENABLED: u32              = 2018;
    pub const TIP_EXCEEDS_MAX_BPS: u32              = 2019;
    pub const MERCHANT_VOLUME_CAPPED: u32           = 2020;
    pub const SLIPPAGE_EXCEEDED: u32                = 2021;
    pub const ORACLE_NOT_WHITELISTED: u32           = 2022;
    pub const CUSTOMER_SPEND_LIMIT_EXCEEDED: u32    = 2023;
    pub const CAPTURE_PAST_DEADLINE: u32            = 2024;
    pub const EVIDENCE_WINDOW_CLOSED: u32           = 2025;
    pub const EVIDENCE_LIMIT_REACHED: u32           = 2026;
    pub const COOLING_OFF_EXPIRED: u32              = 2027;
    pub const NOT_IN_COOLING_OFF: u32               = 2028;
    pub const COOLING_OFF_EXCEEDS_MAX: u32          = 2029;
    pub const PAUSE_COUNT_EXCEEDED: u32             = 2030;
    pub const UNAUTHORIZED_PAUSE: u32               = 2031;
    pub const INSUFFICIENT_MERCHANT_RESERVE: u32    = 2032;
    pub const KYB_VERIFICATION_REQUIRED: u32        = 2033;
    pub const RETRY_NOT_DUE: u32                    = 2034;
    pub const DEBIT_RECORD_NOT_FOUND: u32           = 2035;
    pub const DEBIT_ALREADY_ABANDONED: u32          = 2036;
    pub const DEBIT_ALREADY_SUCCEEDED: u32          = 2037;
    pub const INVALID_PAYMENT_STATUS: u32           = 2038;
    pub const MAX_EXTENSIONS_REACHED: u32           = 2039;
    pub const MAX_EXTENSION_LEDGERS_EXCEEDED: u32   = 2040;
    pub const CUSTOMER_BLOCKED: u32                 = 2050;
    pub const DAO_NOT_CONFIGURED: u32               = 2051;
    pub const NOT_A_DAO_MEMBER: u32                 = 2052;
    pub const DAO_ALREADY_ESCALATED: u32            = 2053;
    pub const DAO_VOTE_WINDOW_OPEN: u32             = 2054;
    pub const DAO_VOTE_WINDOW_CLOSED: u32           = 2055;
    pub const DAO_ALREADY_VOTED: u32                = 2056;
}

// ---------------------------------------------------------------------------
// ahjoor-escrow (3000–3299)
// ---------------------------------------------------------------------------

pub mod escrow {
    pub const INVALID_DEADLINE: u32        = 3001;
    pub const INVALID_TRANCHE_INDEX: u32   = 3002;
    pub const TRANCHE_ALREADY_CLAIMED: u32 = 3003;
}

// ---------------------------------------------------------------------------
// ahjoor-refund (4000–4099)
// ---------------------------------------------------------------------------

pub mod refund {
    // Refund contract uses panic! rather than a contracterror enum;
    // these codes are the off-chain namespace assignments for future migration.
    pub const ALREADY_INITIALIZED: u32             = 4001;
    pub const FEE_EXCEEDS_MAXIMUM: u32             = 4002;
    pub const AMOUNT_MUST_BE_POSITIVE: u32         = 4003;
    pub const INVALID_REASON_CODE: u32             = 4004;
    pub const REFUND_COOLDOWN_ACTIVE: u32          = 4005;
    pub const PAYMENT_NOT_FOUND: u32               = 4006;
    pub const PAYMENT_NOT_COMPLETED: u32           = 4007;
    pub const EXCEEDS_REFUNDABLE_AMOUNT: u32       = 4008;
}

// ---------------------------------------------------------------------------
// ahjoor-token-whitelist (5000–5099)
// ---------------------------------------------------------------------------

pub mod whitelist {
    pub const NOT_INITIALIZED: u32            = 5001;
    pub const ALREADY_INITIALIZED: u32        = 5002;
    pub const UNAUTHORIZED: u32               = 5003;
    pub const TOKEN_ALREADY_WHITELISTED: u32  = 5004;
    pub const TOKEN_NOT_WHITELISTED: u32      = 5005;
    pub const QUOTA_EXCEEDED: u32             = 5006;
    pub const TOKEN_ALREADY_HAS_QUOTA: u32    = 5007;
    pub const TOKEN_HAS_NO_QUOTA: u32         = 5008;
}

// ---------------------------------------------------------------------------
// Convenience: machine-readable error descriptor
// ---------------------------------------------------------------------------

/// Compact descriptor for one error code entry (used in errors.json generation).
pub struct ErrorEntry {
    pub code: u32,
    pub name: &'static str,
    pub contract: &'static str,
}

pub static ALL_ERRORS: &[ErrorEntry] = &[
    // rosca
    ErrorEntry { code: rosca::ALREADY_INITIALIZED, name: "AlreadyInitialized", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::TOKEN_NOT_APPROVED, name: "TokenNotApproved", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CUSTOM_ORDER_LENGTH_MISMATCH, name: "CustomOrderLengthMismatch", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CUSTOM_ORDER_NON_MEMBER, name: "CustomOrderNonMember", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::AMOUNT_MUST_BE_POSITIVE, name: "AmountMustBePositive", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ROUND_DEADLINE_PASSED, name: "RoundDeadlinePassed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::MEMBER_HAS_EXITED, name: "MemberHasExited", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::NOT_A_MEMBER, name: "NotAMember", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::ALREADY_CONTRIBUTED, name: "AlreadyContributed", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::CONTRACT_PAUSED, name: "ContractPaused", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::GROUP_FULL, name: "GroupFull", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::GROUP_FROZEN, name: "GroupFrozen", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::AUCTION_NOT_ENABLED, name: "AuctionNotEnabled", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::MIGRATION_NOT_FOUND, name: "MigrationNotFound", contract: "ahjoor-rosca" },
    ErrorEntry { code: rosca::PROXY_ROUNDS_EXHAUSTED, name: "ProxyRoundsExhausted", contract: "ahjoor-rosca" },
    // payments
    ErrorEntry { code: payments::RATE_LIMIT_EXCEEDED, name: "RateLimitExceeded", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::TOKEN_NOT_ALLOWED, name: "TokenNotAllowed", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::VOUCHER_EXPIRED, name: "VoucherExpired", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::CUSTOMER_BLOCKED, name: "CustomerBlocked", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::KYB_VERIFICATION_REQUIRED, name: "KYBVerificationRequired", contract: "ahjoor-payments" },
    ErrorEntry { code: payments::DAO_NOT_CONFIGURED, name: "DaoNotConfigured", contract: "ahjoor-payments" },
    // escrow
    ErrorEntry { code: escrow::INVALID_DEADLINE, name: "InvalidDeadline", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::INVALID_TRANCHE_INDEX, name: "InvalidTrancheIndex", contract: "ahjoor-escrow" },
    ErrorEntry { code: escrow::TRANCHE_ALREADY_CLAIMED, name: "TrancheAlreadyClaimed", contract: "ahjoor-escrow" },
    // refund
    ErrorEntry { code: refund::ALREADY_INITIALIZED, name: "AlreadyInitialized", contract: "ahjoor-refund" },
    ErrorEntry { code: refund::AMOUNT_MUST_BE_POSITIVE, name: "AmountMustBePositive", contract: "ahjoor-refund" },
    // whitelist
    ErrorEntry { code: whitelist::NOT_INITIALIZED, name: "NotInitialized", contract: "ahjoor-token-whitelist" },
    ErrorEntry { code: whitelist::TOKEN_ALREADY_WHITELISTED, name: "TokenAlreadyWhitelisted", contract: "ahjoor-token-whitelist" },
    ErrorEntry { code: whitelist::TOKEN_NOT_WHITELISTED, name: "TokenNotWhitelisted", contract: "ahjoor-token-whitelist" },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicate_codes() {
        let mut seen = std::vec::Vec::new();
        for entry in ALL_ERRORS {
            assert!(
                !seen.contains(&entry.code),
                "Duplicate error code {} ({}::{})",
                entry.code,
                entry.contract,
                entry.name,
            );
            seen.push(entry.code);
        }
    }

    #[test]
    fn codes_within_contract_ranges() {
        for entry in ALL_ERRORS {
            let in_range = match entry.contract {
                "ahjoor-rosca"            => (1000..=1299).contains(&entry.code),
                "ahjoor-payments"         => (2000..=2299).contains(&entry.code),
                "ahjoor-escrow"           => (3000..=3299).contains(&entry.code),
                "ahjoor-refund"           => (4000..=4099).contains(&entry.code),
                "ahjoor-token-whitelist"  => (5000..=5099).contains(&entry.code),
                _                         => false,
            };
            assert!(
                in_range,
                "Error code {} ({}) is outside the expected range for {}",
                entry.code,
                entry.name,
                entry.contract,
            );
        }
    }
}
