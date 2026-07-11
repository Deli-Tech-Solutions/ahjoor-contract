#![allow(dead_code)]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Bytes,
    BytesN, Env, Map, String, Symbol, Vec,
};
use crate::pre_approved_spending::*;

// Storage key symbols (used as tuple key prefixes to avoid format!)
const ALLOWANCE_COUNTER_KEY: &str = "allowance_counter";
const CONSENT_COUNTER_KEY: &str = "consent_counter";
const TRANSACTION_COUNTER_KEY: &str = "transaction_counter";
const AUDIT_LOG_COUNTER_KEY: &str = "audit_log_counter";

fn allowance_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "allowance"), id)
}
fn consent_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "consent"), id)
}
fn transaction_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "transaction"), id)
}
fn audit_log_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "audit_log"), id)
}
fn customer_allowances_key(env: &Env, customer: &Address) -> (Symbol, Address) {
    (Symbol::new(env, "cust_allows"), customer.clone())
}
fn merchant_allowances_key(env: &Env, merchant: &Address) -> (Symbol, Address) {
    (Symbol::new(env, "merch_allows"), merchant.clone())
}

/// Implementation of pre-approved spending functionality
pub struct PreApprovedSpendingImpl;

impl PreApprovedSpendingImpl {
    /// Create a new spending allowance with consent
    pub fn create_allowance(
        env: &Env,
        customer: Address,
        merchant: Address,
        token: Address,
        total_amount: i128,
        per_transaction_limit: i128,
        daily_limit: i128,
        expires_at: u64,
        consent_hash: BytesN<32>,
        consent_metadata: Map<String, String>,
    ) -> u32 {
        customer.require_auth();

        if total_amount <= 0 || per_transaction_limit <= 0 || daily_limit <= 0 {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        }

        if per_transaction_limit > total_amount || daily_limit > total_amount {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        }

        let now = env.ledger().timestamp();

        if expires_at <= now {
            panic_with_error!(env, SpendingAllowanceError::AllowanceExpired);
        }

        // Get next allowance ID
        let allowance_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, ALLOWANCE_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_id = allowance_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        });

        let allowance = SpendingAllowance {
            allowance_id: next_id,
            customer: customer.clone(),
            merchant: merchant.clone(),
            token,
            total_amount,
            amount_spent: 0,
            created_at: now,
            expires_at,
            status: AllowanceStatus::Active,
            consent_hash,
            consent_timestamp: now,
            consent_metadata,
            per_transaction_limit,
            daily_limit,
            daily_spent: 0,
            daily_reset_timestamp: now,
        };

        // Store allowance
        env.storage().persistent().set(&allowance_key(env, next_id), &allowance);
        env.storage()
            .instance()
            .set(&Symbol::new(env, ALLOWANCE_COUNTER_KEY), &next_id);

        // Add to customer allowances list
        let ckey = customer_allowances_key(env, &customer);
        let mut customer_allowances: Vec<u32> = env
            .storage()
            .persistent()
            .get(&ckey)
            .unwrap_or_else(|| Vec::new(env));
        customer_allowances.push_back(next_id);
        env.storage().persistent().set(&ckey, &customer_allowances);

        // Add to merchant allowances list
        let mkey = merchant_allowances_key(env, &merchant);
        let mut merchant_allowances: Vec<u32> = env
            .storage()
            .persistent()
            .get(&mkey)
            .unwrap_or_else(|| Vec::new(env));
        merchant_allowances.push_back(next_id);
        env.storage().persistent().set(&mkey, &merchant_allowances);

        // Log audit entry
        Self::log_audit_entry(
            env,
            next_id,
            AuditAction::Created,
            customer.clone(),
            "Allowance created",
        );

        next_id
    }

    /// Record consent for an allowance
    pub fn record_consent(
        env: &Env,
        customer: Address,
        merchant: Address,
        consent_type: ConsentType,
        consent_hash: BytesN<32>,
        ip_hash: BytesN<32>,
        device_hash: BytesN<32>,
        location_hash: BytesN<32>,
        expires_at: u64,
        metadata: Map<String, String>,
    ) -> u32 {
        customer.require_auth();

        let now = env.ledger().timestamp();

        if expires_at <= now {
            panic_with_error!(env, SpendingAllowanceError::ConsentExpired);
        }

        // Get next consent ID
        let consent_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, CONSENT_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_id = consent_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        });

        let consent = ConsentRecord {
            consent_id: next_id,
            customer,
            merchant,
            consent_type,
            consent_hash,
            timestamp: now,
            expires_at,
            ip_hash,
            device_hash,
            location_hash,
            status: ConsentStatus::Active,
            metadata,
        };

        // Store consent
        env.storage().persistent().set(&consent_key(env, next_id), &consent);
        env.storage()
            .instance()
            .set(&Symbol::new(env, CONSENT_COUNTER_KEY), &next_id);

        next_id
    }

    /// Spend from an allowance
    pub fn spend_from_allowance(
        env: &Env,
        allowance_id: u32,
        amount: i128,
        reference: String,
    ) -> AllowanceTransaction {
        let key = allowance_key(env, allowance_id);
        let mut allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        let now = env.ledger().timestamp();

        // Check allowance status
        match allowance.status {
            AllowanceStatus::Revoked => {
                panic_with_error!(env, SpendingAllowanceError::AllowanceRevoked);
            }
            AllowanceStatus::Paused => {
                panic_with_error!(env, SpendingAllowanceError::AllowancePaused);
            }
            AllowanceStatus::Expired => {
                panic_with_error!(env, SpendingAllowanceError::AllowanceExpired);
            }
            AllowanceStatus::Exhausted => {
                panic_with_error!(env, SpendingAllowanceError::AllowanceExhausted);
            }
            _ => {}
        }

        // Check expiration
        if now > allowance.expires_at {
            allowance.status = AllowanceStatus::Expired;
            env.storage().persistent().set(&key, &allowance);
            panic_with_error!(env, SpendingAllowanceError::AllowanceExpired);
        }

        // Check per-transaction limit
        if amount > allowance.per_transaction_limit {
            panic_with_error!(env, SpendingAllowanceError::PerTransactionLimitExceeded);
        }

        // Reset daily limit if needed
        let day_in_seconds: u64 = 24 * 60 * 60;
        if now > allowance.daily_reset_timestamp + day_in_seconds {
            allowance.daily_spent = 0;
            allowance.daily_reset_timestamp = now;
        }

        // Check daily limit
        let new_daily_spent = allowance
            .daily_spent
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount));

        if new_daily_spent > allowance.daily_limit {
            panic_with_error!(env, SpendingAllowanceError::DailyLimitExceeded);
        }

        // Check total limit
        let new_total_spent = allowance
            .amount_spent
            .checked_add(amount)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount));

        if new_total_spent > allowance.total_amount {
            panic_with_error!(env, SpendingAllowanceError::AllowanceExhausted);
        }

        // Update allowance
        allowance.amount_spent = new_total_spent;
        allowance.daily_spent = new_daily_spent;

        if new_total_spent >= allowance.total_amount {
            allowance.status = AllowanceStatus::Exhausted;
        }

        // Get transaction ID
        let tx_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, TRANSACTION_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_tx_id = tx_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        });

        let transaction = AllowanceTransaction {
            tx_id: next_tx_id,
            allowance_id,
            amount,
            timestamp: now,
            status: TransactionStatus::Completed,
            reference,
        };

        // Store transaction
        env.storage().persistent().set(&transaction_key(env, next_tx_id), &transaction);

        // Store updated allowance
        env.storage().persistent().set(&key, &allowance);
        env.storage()
            .instance()
            .set(&Symbol::new(env, TRANSACTION_COUNTER_KEY), &next_tx_id);

        // Log audit entry
        Self::log_audit_entry(
            env,
            allowance_id,
            AuditAction::TransactionApproved,
            allowance.customer.clone(),
            "Spent from allowance",
        );

        transaction
    }

    /// Get allowance details
    pub fn get_allowance(env: &Env, allowance_id: u32) -> Option<SpendingAllowance> {
        env.storage().persistent().get(&allowance_key(env, allowance_id))
    }

    /// Get consent record
    pub fn get_consent(env: &Env, consent_id: u32) -> Option<ConsentRecord> {
        env.storage().persistent().get(&consent_key(env, consent_id))
    }

    /// Pause an allowance
    pub fn pause_allowance(env: &Env, allowance_id: u32) {
        let key = allowance_key(env, allowance_id);
        let mut allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance.customer.require_auth();

        allowance.status = AllowanceStatus::Paused;
        env.storage().persistent().set(&key, &allowance);

        Self::log_audit_entry(
            env,
            allowance_id,
            AuditAction::Paused,
            allowance.customer.clone(),
            "Allowance paused",
        );
    }

    /// Resume a paused allowance
    pub fn resume_allowance(env: &Env, allowance_id: u32) {
        let key = allowance_key(env, allowance_id);
        let mut allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance.customer.require_auth();

        if allowance.status != AllowanceStatus::Paused {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        }

        allowance.status = AllowanceStatus::Active;
        env.storage().persistent().set(&key, &allowance);

        Self::log_audit_entry(
            env,
            allowance_id,
            AuditAction::Resumed,
            allowance.customer.clone(),
            "Allowance resumed",
        );
    }

    /// Revoke an allowance
    pub fn revoke_allowance(env: &Env, allowance_id: u32) {
        let key = allowance_key(env, allowance_id);
        let mut allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance.customer.require_auth();

        allowance.status = AllowanceStatus::Revoked;
        env.storage().persistent().set(&key, &allowance);

        Self::log_audit_entry(
            env,
            allowance_id,
            AuditAction::Revoked,
            allowance.customer.clone(),
            "Allowance revoked",
        );
    }

    /// Revoke consent
    pub fn revoke_consent(env: &Env, consent_id: u32) {
        let key = consent_key(env, consent_id);
        let mut consent: ConsentRecord = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::ConsentNotFound));

        consent.customer.require_auth();

        consent.status = ConsentStatus::Revoked;
        env.storage().persistent().set(&key, &consent);
    }

    /// Get remaining balance
    pub fn get_remaining_balance(env: &Env, allowance_id: u32) -> i128 {
        let allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&allowance_key(env, allowance_id))
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance
            .total_amount
            .checked_sub(allowance.amount_spent)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount))
    }

    /// Get daily remaining balance
    pub fn get_daily_remaining(env: &Env, allowance_id: u32) -> i128 {
        let allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&allowance_key(env, allowance_id))
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance
            .daily_limit
            .checked_sub(allowance.daily_spent)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount))
    }

    /// Get allowance transaction history
    pub fn get_allowance_transactions(env: &Env, _allowance_id: u32) -> Vec<AllowanceTransaction> {
        Vec::new(env)
    }

    /// Get audit log for an allowance
    pub fn get_audit_log(env: &Env, _allowance_id: u32) -> Vec<AllowanceAuditLog> {
        Vec::new(env)
    }

    /// Get all allowances for a customer
    pub fn get_customer_allowances(env: &Env, customer: Address) -> Vec<SpendingAllowance> {
        let mut allowances = Vec::new(env);
        let ckey = customer_allowances_key(env, &customer);

        if let Some(allowance_ids) = env.storage().persistent().get::<_, Vec<u32>>(&ckey) {
            for id in allowance_ids.iter() {
                if let Some(allowance) = env.storage().persistent().get::<_, SpendingAllowance>(&allowance_key(env, id)) {
                    allowances.push_back(allowance);
                }
            }
        }

        allowances
    }

    /// Get all allowances for a merchant
    pub fn get_merchant_allowances(env: &Env, merchant: Address) -> Vec<SpendingAllowance> {
        let mut allowances = Vec::new(env);
        let mkey = merchant_allowances_key(env, &merchant);

        if let Some(allowance_ids) = env.storage().persistent().get::<_, Vec<u32>>(&mkey) {
            for id in allowance_ids.iter() {
                if let Some(allowance) = env.storage().persistent().get::<_, SpendingAllowance>(&allowance_key(env, id)) {
                    allowances.push_back(allowance);
                }
            }
        }

        allowances
    }

    /// Verify consent is valid
    pub fn verify_consent(env: &Env, consent_id: u32) -> bool {
        if let Some(consent) = env.storage().persistent().get::<_, ConsentRecord>(&consent_key(env, consent_id)) {
            let now = env.ledger().timestamp();
            consent.status == ConsentStatus::Active && now <= consent.expires_at
        } else {
            false
        }
    }

    /// Update allowance limits
    pub fn update_allowance_limits(
        env: &Env,
        allowance_id: u32,
        per_transaction_limit: i128,
        daily_limit: i128,
    ) {
        let key = allowance_key(env, allowance_id);
        let mut allowance: SpendingAllowance = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, SpendingAllowanceError::AllowanceNotFound));

        allowance.customer.require_auth();

        if per_transaction_limit <= 0 || daily_limit <= 0 {
            panic_with_error!(env, SpendingAllowanceError::InvalidAllowanceAmount);
        }

        allowance.per_transaction_limit = per_transaction_limit;
        allowance.daily_limit = daily_limit;
        env.storage().persistent().set(&key, &allowance);

        Self::log_audit_entry(
            env,
            allowance_id,
            AuditAction::Modified,
            allowance.customer.clone(),
            "Allowance limits updated",
        );
    }

    /// Helper function to log audit entries
    fn log_audit_entry(
        env: &Env,
        allowance_id: u32,
        action: AuditAction,
        actor: Address,
        details: &str,
    ) {
        let log_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, AUDIT_LOG_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_log_id = log_id.checked_add(1).unwrap_or(log_id);

        let log = AllowanceAuditLog {
            log_id: next_log_id,
            allowance_id,
            action,
            actor,
            timestamp: env.ledger().timestamp(),
            details: String::from_str(env, details),
        };

        env.storage().persistent().set(&audit_log_key(env, next_log_id), &log);
        env.storage()
            .instance()
            .set(&Symbol::new(env, AUDIT_LOG_COUNTER_KEY), &next_log_id);
    }
}
