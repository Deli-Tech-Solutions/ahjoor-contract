#![allow(dead_code)]
use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, panic_with_error, token, Address, Bytes,
    BytesN, Env, Map, String, Symbol, Vec,
};
use crate::multi_token_invoice::*;

// Storage keys
const INVOICE_COUNTER_KEY: &str = "invoice_counter";
const INVOICE_KEY_PREFIX: &str = "invoice_";
const INVOICE_PAYMENT_KEY_PREFIX: &str = "invoice_payment_";
const SETTLEMENT_BATCH_KEY_PREFIX: &str = "settlement_batch_";
const SETTLEMENT_BATCH_COUNTER_KEY: &str = "settlement_batch_counter";
const PAYMENT_COUNTER_KEY: &str = "payment_counter";
const MERCHANT_CONVERSION_RATES_KEY_PREFIX: &str = "merchant_rates_";

fn invoice_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "invoice"), id)
}
fn inv_payment_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "inv_payment"), id)
}
fn settle_batch_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "settle_batch"), id)
}
fn cross_settle_key(env: &Env, id: u32) -> (Symbol, u32) {
    (Symbol::new(env, "cross_settle"), id)
}
fn merch_rates_key(env: &Env, merchant: &Address) -> (Symbol, Address) {
    (Symbol::new(env, "merch_rates"), merchant.clone())
}

/// Implementation of multi-token invoice functionality
pub struct MultiTokenInvoiceImpl;

impl MultiTokenInvoiceImpl {
    /// Create a new multi-token invoice
    pub fn create_invoice(
        env: &Env,
        merchant: Address,
        customer: Address,
        total_amount: i128,
        base_currency: Address,
        accepted_tokens: Vec<Address>,
        preferred_settlement_token: Address,
        line_items: Vec<InvoiceLineItem>,
        due_date: u64,
        metadata: Map<String, String>,
    ) -> u32 {
        Self::create_invoice_with_oracle(
            env,
            merchant,
            customer,
            total_amount,
            base_currency,
            accepted_tokens,
            preferred_settlement_token,
            line_items,
            due_date,
            metadata,
            None,
        )
    }

    /// Create a new multi-token invoice with an optional oracle contract for
    /// cross-token settlement price discovery (#354).
    pub fn create_invoice_with_oracle(
        env: &Env,
        merchant: Address,
        customer: Address,
        total_amount: i128,
        base_currency: Address,
        accepted_tokens: Vec<Address>,
        preferred_settlement_token: Address,
        line_items: Vec<InvoiceLineItem>,
        due_date: u64,
        metadata: Map<String, String>,
        oracle_contract: Option<Address>,
    ) -> u32 {
        merchant.require_auth();

        if total_amount <= 0 {
            panic_with_error!(env, MultiTokenInvoiceError::InvalidLineItem);
        }

        if line_items.len() > 20 {
            panic_with_error!(env, MultiTokenInvoiceError::InvalidLineItem);
        }

        // Validate line items
        let mut calculated_total: i128 = 0;
        for item in line_items.iter() {
            if item.quantity <= 0 || item.unit_price <= 0 {
                panic_with_error!(env, MultiTokenInvoiceError::InvalidLineItem);
            }
            calculated_total = calculated_total
                .checked_add(item.amount)
                .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvalidLineItem));
        }

        // Get next invoice ID
        let invoice_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, INVOICE_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_id = invoice_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, MultiTokenInvoiceError::SettlementFailed);
        });

        let now = env.ledger().timestamp();

        let invoice = MultiTokenInvoice {
            invoice_id: next_id,
            merchant: merchant.clone(),
            customer: customer.clone(),
            created_at: now,
            due_date,
            total_amount,
            base_currency: base_currency.clone(),
            accepted_tokens,
            preferred_settlement_token,
            line_items,
            status: InvoiceStatus::Issued,
            payments_received: Map::new(env),
            conversion_rates: Map::new(env),
            settlement_conversion_rate: 1_000_000, // 1:1 by default
            oracle_contract,
            metadata,
        };

        // Store invoice
        let key = invoice_key(env, next_id);
        env.storage().persistent().set(&key, &invoice);
        env.storage()
            .instance()
            .set(&Symbol::new(env, INVOICE_COUNTER_KEY), &next_id);

        next_id
    }

    /// Accept payment for an invoice
    pub fn accept_payment(
        env: &Env,
        invoice_id: u32,
        payer: Address,
        token: Address,
        amount: i128,
    ) -> InvoicePayment {
        payer.require_auth();

        if amount <= 0 {
            panic_with_error!(env, MultiTokenInvoiceError::InvalidLineItem);
        }

        // Get invoice
        let key = invoice_key(env, invoice_id);
        let mut invoice: MultiTokenInvoice = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvoiceNotFound));

        // Check invoice status
        match invoice.status {
            InvoiceStatus::Cancelled => {
                panic_with_error!(env, MultiTokenInvoiceError::InvalidInvoiceStatus);
            }
            InvoiceStatus::FullyPaid => {
                panic_with_error!(env, MultiTokenInvoiceError::PaymentExceedsInvoiceAmount);
            }
            _ => {}
        }

        // Check if token is accepted
        let mut token_accepted = false;
        for accepted_token in invoice.accepted_tokens.iter() {
            if accepted_token == token {
                token_accepted = true;
                break;
            }
        }

        if !token_accepted {
            panic_with_error!(env, MultiTokenInvoiceError::TokenNotAccepted);
        }

        // Get conversion rate
        let conversion_rate: i128 = invoice
            .conversion_rates
            .get(token.clone())
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::ConversionRateNotSet));

        // Calculate amount in base currency
        let amount_in_base = amount
            .checked_mul(conversion_rate)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvalidConversionRate))
            .checked_div(1_000_000)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvalidConversionRate));

        // Calculate amount in settlement currency
        let amount_in_settlement = amount_in_base
            .checked_mul(invoice.settlement_conversion_rate)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvalidConversionRate))
            .checked_div(1_000_000)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvalidConversionRate));

        // Update payments received
        let current_payment: i128 = invoice
            .payments_received
            .get(token.clone())
            .unwrap_or(0);

        let new_payment = current_payment
            .checked_add(amount_in_base)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::PaymentExceedsInvoiceAmount));

        if new_payment > invoice.total_amount {
            panic_with_error!(env, MultiTokenInvoiceError::PaymentExceedsInvoiceAmount);
        }

        invoice.payments_received.set(token.clone(), new_payment);

        // Update invoice status
        if new_payment >= invoice.total_amount {
            invoice.status = InvoiceStatus::FullyPaid;
        } else {
            invoice.status = InvoiceStatus::PartiallyPaid;
        }

        // Get payment ID
        let payment_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, PAYMENT_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_payment_id = payment_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, MultiTokenInvoiceError::SettlementFailed);
        });

        let now = env.ledger().timestamp();

        let payment = InvoicePayment {
            payment_id: next_payment_id,
            invoice_id,
            token,
            amount,
            amount_in_base,
            amount_in_settlement,
            paid_at: now,
            payer,
            tx_hash: BytesN::from_array(env, &[0u8; 32]),
        };

        // Store payment
        let payment_key = inv_payment_key(env, next_payment_id);
        env.storage().persistent().set(&payment_key, &payment);

        // Store updated invoice
        env.storage().persistent().set(&key, &invoice);
        env.storage()
            .instance()
            .set(&Symbol::new(env, PAYMENT_COUNTER_KEY), &next_payment_id);

        payment
    }

    /// Set conversion rate for a token
    pub fn set_conversion_rate(
        env: &Env,
        merchant: Address,
        token: Address,
        rate_to_base: i128,
    ) {
        merchant.require_auth();

        if rate_to_base <= 0 {
            panic_with_error!(env, MultiTokenInvoiceError::InvalidConversionRate);
        }

        let key = merch_rates_key(env, &merchant);
        let mut rates: Map<Address, i128> = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| Map::new(env));

        rates.set(token, rate_to_base);
        env.storage().persistent().set(&key, &rates);
    }

    /// Set settlement conversion rate
    pub fn set_settlement_conversion_rate(
        env: &Env,
        merchant: Address,
        invoice_id: u32,
        rate: i128,
    ) {
        merchant.require_auth();

        if rate <= 0 {
            panic_with_error!(env, MultiTokenInvoiceError::InvalidConversionRate);
        }

        let key = invoice_key(env, invoice_id);
        let mut invoice: MultiTokenInvoice = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvoiceNotFound));

        if invoice.merchant != merchant {
            panic_with_error!(env, MultiTokenInvoiceError::UnauthorizedAccess);
        }

        invoice.settlement_conversion_rate = rate;
        env.storage().persistent().set(&key, &invoice);
    }

    /// Get invoice details
    pub fn get_invoice(env: &Env, invoice_id: u32) -> Option<MultiTokenInvoice> {
        let key = invoice_key(env, invoice_id);
        env.storage().persistent().get(&key)
    }

    /// Get invoice payment history
    pub fn get_invoice_payments(env: &Env, invoice_id: u32) -> Vec<InvoicePayment> {
        let mut payments = Vec::new(env);

        // This would require iterating through all payments and filtering by invoice_id
        // For now, return empty vector - in production, use a proper indexing strategy
        payments
    }

    /// Settle invoices in batch
    pub fn settle_invoices(
        env: &Env,
        merchant: Address,
        invoice_ids: Vec<u32>,
    ) -> SettlementBatch {
        merchant.require_auth();

        if invoice_ids.len() > 50 {
            panic_with_error!(env, MultiTokenInvoiceError::SettlementFailed);
        }

        let mut total_settlement_amount: i128 = 0;

        for invoice_id in invoice_ids.iter() {
            let key = invoice_key(env, invoice_id);
            let invoice: MultiTokenInvoice = env
                .storage()
                .persistent()
                .get(&key)
                .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvoiceNotFound));

            if invoice.merchant != merchant {
                panic_with_error!(env, MultiTokenInvoiceError::UnauthorizedAccess);
            }

            if invoice.status != InvoiceStatus::FullyPaid {
                panic_with_error!(env, MultiTokenInvoiceError::InvalidInvoiceStatus);
            }

            total_settlement_amount = total_settlement_amount
                .checked_add(invoice.total_amount)
                .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::SettlementFailed));
        }

        // Get batch ID
        let batch_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, SETTLEMENT_BATCH_COUNTER_KEY))
            .unwrap_or(0u32);

        let next_batch_id = batch_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, MultiTokenInvoiceError::SettlementFailed);
        });

        let now = env.ledger().timestamp();

        let batch = SettlementBatch {
            batch_id: next_batch_id,
            invoice_ids,
            total_settlement_amount,
            settled_at: now,
            status: SettlementStatus::Completed,
            merchant,
        };

        // Store batch
        let batch_key = settle_batch_key(env, next_batch_id);
        env.storage().persistent().set(&batch_key, &batch);
        env.storage()
            .instance()
            .set(&Symbol::new(env, SETTLEMENT_BATCH_COUNTER_KEY), &next_batch_id);

        batch
    }

    /// Get settlement batch details
    pub fn get_settlement_batch(env: &Env, batch_id: u32) -> Option<SettlementBatch> {
        let key = settle_batch_key(env, batch_id);
        env.storage().persistent().get(&key)
    }

    /// Cancel an invoice
    pub fn cancel_invoice(env: &Env, invoice_id: u32) {
        let key = invoice_key(env, invoice_id);
        let mut invoice: MultiTokenInvoice = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvoiceNotFound));

        invoice.merchant.require_auth();

        invoice.status = InvoiceStatus::Cancelled;
        env.storage().persistent().set(&key, &invoice);
    }

    /// Get invoice status
    pub fn get_invoice_status(env: &Env, invoice_id: u32) -> InvoiceStatus {
        let key = invoice_key(env, invoice_id);
        let invoice: MultiTokenInvoice = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvoiceNotFound));

        invoice.status
    }

    /// Pay an invoice using a different token than the invoice currency, using an oracle
    /// for price discovery and slippage validation (#354).
    ///
    /// The invoice must have an `oracle_contract` configured. The oracle is queried for
    /// the exchange rate between `payment_token` and the invoice's `base_currency`. If
    /// the implied deviation from expected exceeds `max_slippage_bps`, the call is
    /// rejected. On success, `payment_amount` of `payment_token` is transferred from
    /// `payer` and the oracle-derived equivalent is credited against the invoice total.
    /// A `CrossTokenSettlement` event is emitted.
    pub fn pay_invoice_cross_token(
        env: &Env,
        invoice_id: u32,
        payer: Address,
        payment_token: Address,
        payment_amount: i128,
        max_slippage_bps: u32,
    ) -> InvoicePayment {
        use crate::multi_token_invoice::InvoiceOracleClient;

        payer.require_auth();

        if payment_amount <= 0 {
            panic_with_error!(env, MultiTokenInvoiceError::InvalidLineItem);
        }

        let key = invoice_key(env, invoice_id);
        let mut invoice: MultiTokenInvoice = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvoiceNotFound));

        match invoice.status {
            InvoiceStatus::Cancelled => {
                panic_with_error!(env, MultiTokenInvoiceError::InvalidInvoiceStatus);
            }
            InvoiceStatus::FullyPaid => {
                panic_with_error!(env, MultiTokenInvoiceError::PaymentExceedsInvoiceAmount);
            }
            _ => {}
        }

        // Validate that the payment token is in the accepted list
        let mut token_accepted = false;
        for accepted_token in invoice.accepted_tokens.iter() {
            if accepted_token == payment_token {
                token_accepted = true;
                break;
            }
        }
        if !token_accepted {
            panic_with_error!(env, MultiTokenInvoiceError::TokenNotAccepted);
        }

        // Require oracle to be configured
        let oracle_addr = invoice
            .oracle_contract
            .clone()
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::OracleNotConfigured));

        // Query oracle: price of payment_token in terms of base_currency (scaled × 1_000_000)
        let oracle_client = InvoiceOracleClient::new(env, &oracle_addr);
        let oracle_price: i128 = oracle_client
            .get_price(&payment_token, &invoice.base_currency)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::OraclePriceUnavailable));

        if oracle_price <= 0 {
            panic_with_error!(env, MultiTokenInvoiceError::OraclePriceUnavailable);
        }

        // Compute the base-currency equivalent of the payment
        let amount_in_base = payment_amount
            .checked_mul(oracle_price)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvalidConversionRate))
            .checked_div(1_000_000)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvalidConversionRate));

        // Slippage check: ensure oracle price does not deviate from stored conversion rate
        // by more than max_slippage_bps relative to the stored rate.
        if let Some(stored_rate) = invoice.conversion_rates.get(payment_token.clone()) {
            if stored_rate > 0 {
                let diff = if oracle_price > stored_rate {
                    oracle_price - stored_rate
                } else {
                    stored_rate - oracle_price
                };
                // diff / stored_rate > max_slippage_bps / 10_000
                if diff.checked_mul(10_000).unwrap_or(i128::MAX)
                    > stored_rate.checked_mul(max_slippage_bps as i128).unwrap_or(i128::MAX)
                {
                    panic_with_error!(env, MultiTokenInvoiceError::SlippageExceeded);
                }
            }
        }

        // Validate payment does not exceed remaining invoice balance
        let current_paid: i128 = invoice
            .payments_received
            .get(payment_token.clone())
            .unwrap_or(0);
        let new_paid = current_paid
            .checked_add(amount_in_base)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::PaymentExceedsInvoiceAmount));

        if new_paid > invoice.total_amount {
            panic_with_error!(env, MultiTokenInvoiceError::PaymentExceedsInvoiceAmount);
        }

        // Transfer payment_token from payer to contract
        let token_client = token::Client::new(env, &payment_token);
        token_client.transfer(&payer, &env.current_contract_address(), &payment_amount);

        invoice.payments_received.set(payment_token.clone(), new_paid);

        if new_paid >= invoice.total_amount {
            invoice.status = InvoiceStatus::FullyPaid;
        } else {
            invoice.status = InvoiceStatus::PartiallyPaid;
        }

        let amount_in_settlement = amount_in_base
            .checked_mul(invoice.settlement_conversion_rate)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvalidConversionRate))
            .checked_div(1_000_000)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvalidConversionRate));

        let payment_id: u32 = env
            .storage()
            .instance()
            .get(&Symbol::new(env, PAYMENT_COUNTER_KEY))
            .unwrap_or(0u32);
        let next_payment_id = payment_id.checked_add(1).unwrap_or_else(|| {
            panic_with_error!(env, MultiTokenInvoiceError::SettlementFailed)
        });

        let now = env.ledger().timestamp();

        let payment = InvoicePayment {
            payment_id: next_payment_id,
            invoice_id,
            token: payment_token.clone(),
            amount: payment_amount,
            amount_in_base,
            amount_in_settlement,
            paid_at: now,
            payer,
            tx_hash: BytesN::from_array(env, &[0u8; 32]),
        };

        let payment_key = inv_payment_key(env, next_payment_id);
        env.storage().persistent().set(&payment_key, &payment);
        env.storage().persistent().set(&key, &invoice);
        env.storage()
            .instance()
            .set(&Symbol::new(env, PAYMENT_COUNTER_KEY), &next_payment_id);

        // Store cross-token settlement record
        let settlement_key = cross_settle_key(env, invoice_id);
        let settlement_record = CrossTokenSettlementRecord {
            invoice_id,
            paid_token: payment_token.clone(),
            paid_amount: payment_amount,
            invoiced_token: invoice.base_currency.clone(),
            invoiced_amount: amount_in_base,
            oracle_price,
            max_slippage_bps,
        };
        env.storage().persistent().set(&settlement_key, &settlement_record);

        // Emit cross-token settlement event
        env.events().publish(
            (Symbol::new(env, "CrossTokenSettlement"),),
            (
                invoice_id,
                payment_token,
                payment_amount,
                invoice.base_currency,
                amount_in_base,
            ),
        );

        payment
    }

    /// Get remaining balance for an invoice
    pub fn get_invoice_balance(env: &Env, invoice_id: u32) -> i128 {
        let key = invoice_key(env, invoice_id);
        let invoice: MultiTokenInvoice = env
            .storage()
            .persistent()
            .get(&key)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::InvoiceNotFound));

        let mut total_paid: i128 = 0;
        for payment in invoice.payments_received.iter() {
            total_paid = total_paid
                .checked_add(payment.1)
                .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::SettlementFailed));
        }

        invoice
            .total_amount
            .checked_sub(total_paid)
            .unwrap_or_else(|| panic_with_error!(env, MultiTokenInvoiceError::SettlementFailed))
    }
}
