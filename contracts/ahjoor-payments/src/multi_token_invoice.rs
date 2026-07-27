#![allow(dead_code)]
use soroban_sdk::{
    contracterror, contractclient, contracttype, Address,
    BytesN, Env, Map, String, Vec,
};

/// Minimal oracle interface for cross-token price lookups (#354).
#[allow(dead_code)]
#[contractclient(name = "InvoiceOracleClient")]
pub trait InvoiceOracleInterface {
    /// Returns the price of `base` in terms of `quote`, scaled by 1_000_000.
    /// Returns `None` if the price feed is unavailable.
    fn get_price(env: soroban_sdk::Env, base: Address, quote: Address) -> Option<i128>;
}

/// Record of a cross-token oracle settlement for an invoice (#354).
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CrossTokenSettlementRecord {
    /// Invoice ID that was settled.
    pub invoice_id: u32,
    /// Token sent by the payer.
    pub paid_token: Address,
    /// Amount of `paid_token` sent by the payer.
    pub paid_amount: i128,
    /// Token the invoice was denominated in.
    pub invoiced_token: Address,
    /// Equivalent amount in the invoiced token (computed via oracle).
    pub invoiced_amount: i128,
    /// Oracle price used (base/quote, scaled by 1_000_000).
    pub oracle_price: i128,
    /// Slippage tolerance that was checked (in basis points).
    pub max_slippage_bps: u32,
}

/// Multi-token invoice with preferred settlement conversion
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MultiTokenInvoice {
    /// Unique invoice ID
    pub invoice_id: u32,
    /// Merchant address
    pub merchant: Address,
    /// Customer address
    pub customer: Address,
    /// Invoice creation timestamp
    pub created_at: u64,
    /// Invoice due date timestamp
    pub due_date: u64,
    /// Total amount in base currency
    pub total_amount: i128,
    /// Base currency token address
    pub base_currency: Address,
    /// Accepted payment tokens (can be different from base currency)
    pub accepted_tokens: Vec<Address>,
    /// Preferred settlement token (for conversion)
    pub preferred_settlement_token: Address,
    /// Invoice line items
    pub line_items: Vec<InvoiceLineItem>,
    /// Invoice status
    pub status: InvoiceStatus,
    /// Payments received (token -> amount)
    pub payments_received: Map<Address, i128>,
    /// Conversion rates for each accepted token to base currency
    pub conversion_rates: Map<Address, i128>,
    /// Settlement conversion rate (base to preferred settlement)
    pub settlement_conversion_rate: i128,
    /// Optional oracle contract for cross-token settlement (#354).
    /// When set, payers may settle using any accepted token and oracle price lookup
    /// is used to validate the equivalent amount.
    pub oracle_contract: Option<Address>,
    /// Metadata
    pub metadata: Map<String, String>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoiceLineItem {
    pub description: String,
    pub quantity: i128,
    pub unit_price: i128,
    pub amount: i128,
    pub tax_rate_bps: u32,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InvoiceStatus {
    Draft = 0,
    Issued = 1,
    PartiallyPaid = 2,
    FullyPaid = 3,
    Overdue = 4,
    Cancelled = 5,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct InvoicePayment {
    /// Payment ID
    pub payment_id: u32,
    /// Invoice ID
    pub invoice_id: u32,
    /// Token used for payment
    pub token: Address,
    /// Amount paid in the token
    pub amount: i128,
    /// Amount in base currency (after conversion)
    pub amount_in_base: i128,
    /// Amount in settlement currency (after conversion)
    pub amount_in_settlement: i128,
    /// Payment timestamp
    pub paid_at: u64,
    /// Payer address
    pub payer: Address,
    /// Transaction hash for reference
    pub tx_hash: BytesN<32>,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SettlementBatch {
    /// Batch ID
    pub batch_id: u32,
    /// Invoices included in this batch
    pub invoice_ids: Vec<u32>,
    /// Total amount in settlement currency
    pub total_settlement_amount: i128,
    /// Settlement timestamp
    pub settled_at: u64,
    /// Settlement status
    pub status: SettlementStatus,
    /// Merchant receiving settlement
    pub merchant: Address,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SettlementStatus {
    Pending = 0,
    Processing = 1,
    Completed = 2,
    Failed = 3,
}

#[contracterror]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MultiTokenInvoiceError {
    InvoiceNotFound = 1,
    InvalidInvoiceStatus = 2,
    PaymentExceedsInvoiceAmount = 3,
    TokenNotAccepted = 4,
    ConversionRateNotSet = 5,
    InvoiceExpired = 6,
    UnauthorizedAccess = 7,
    InvalidLineItem = 8,
    SettlementFailed = 9,
    InvalidConversionRate = 10,
    SlippageExceeded = 11,
    OracleNotConfigured = 12,
    OraclePriceUnavailable = 13,
}

pub trait MultiTokenInvoiceInterface {
    /// Create a new multi-token invoice
    fn create_invoice(
        env: Env,
        merchant: Address,
        customer: Address,
        total_amount: i128,
        base_currency: Address,
        accepted_tokens: Vec<Address>,
        preferred_settlement_token: Address,
        line_items: Vec<InvoiceLineItem>,
        due_date: u64,
        metadata: Map<String, String>,
    ) -> u32;

    /// Accept payment for an invoice in any accepted token
    fn accept_payment(
        env: Env,
        invoice_id: u32,
        payer: Address,
        token: Address,
        amount: i128,
    ) -> InvoicePayment;

    /// Set conversion rate for a token
    fn set_conversion_rate(
        env: Env,
        merchant: Address,
        token: Address,
        rate_to_base: i128,
    );

    /// Set settlement conversion rate
    fn set_settlement_conversion_rate(
        env: Env,
        merchant: Address,
        invoice_id: u32,
        rate: i128,
    );

    /// Get invoice details
    fn get_invoice(env: Env, invoice_id: u32) -> Option<MultiTokenInvoice>;

    /// Get invoice payment history
    fn get_invoice_payments(env: Env, invoice_id: u32) -> Vec<InvoicePayment>;

    /// Settle invoices in batch with preferred token conversion
    fn settle_invoices(
        env: Env,
        merchant: Address,
        invoice_ids: Vec<u32>,
    ) -> SettlementBatch;

    /// Get settlement batch details
    fn get_settlement_batch(env: Env, batch_id: u32) -> Option<SettlementBatch>;

    /// Cancel an invoice
    fn cancel_invoice(env: Env, invoice_id: u32);

    /// Get invoice status
    fn get_invoice_status(env: Env, invoice_id: u32) -> InvoiceStatus;

    /// Get remaining balance for an invoice
    fn get_invoice_balance(env: Env, invoice_id: u32) -> i128;
}
