# ROSCA Contract Migration Guide

This guide explains how to upgrade existing ROSCA contract deployments to the latest version.

## Step-by-Step Upgrade Process

### 1. Build the new WASM

```bash
make build
```

### 2. Upload new WASM to network

```bash
stellar contract upload \
  --wasm target/wasm32-unknown-unknown/release/ahjoor_rosca.wasm \
  --source admin \
  --network testnet
```

### 3. Upgrade the on-chain contract

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- upgrade --new_wasm_hash <WASM_HASH>
```

### 4. Run migration

```bash
stellar contract invoke \
  --id <CONTRACT_ID> \
  --source admin \
  --network testnet \
  -- migrate
```

## New Storage Keys and Post-Migration Defaults

| Key               | Default After Migration | Notes                                  |
|-------------------|-------------------------|----------------------------------------|
| FeeBps            | 0                       | No fee charged until explicitly set    |
| FeeRecipient      | None                    | Must be set before fees take effect    |
| MaxDefaults       | 3                       | Matches previous hard-coded behavior   |

## Verification Steps

After migration, verify the state by querying:
- `get_fee_bps`
- `get_max_defaults`
- Existing member data to confirm state is intact
