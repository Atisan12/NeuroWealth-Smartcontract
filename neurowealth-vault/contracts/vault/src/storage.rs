use soroban_sdk::{contracttype, Address, Env, Map, Symbol, Vec};

use crate::error::VaultError;

/// Supported decimals range for registered assets.
/// Stellar classic assets use 7 decimals; Soroban token contracts commonly use
/// 6-18. We accept 0..=18 to cover both while rejecting unsupported precision.
pub const MIN_ASSET_DECIMALS: u32 = 0;
pub const MAX_ASSET_DECIMALS: u32 = 18;

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetMetadata {
    pub token: Address,
    pub symbol: Symbol,
    pub decimals: u32,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssetBalance {
    /// Raw token units, exactly as reported by the token contract.
    /// No implicit scaling or rounding is applied.
    pub amount: i128,
    pub decimals: u32,
}

/// Per-holder share accounting that separates locked from unlocked shares.
///
/// `unlocked` shares may be redeemed through any withdrawal entrypoint.
/// `locked` shares are subject to a share lock and may only be released by an
/// entrypoint that explicitly documents a lock bypass (emergency withdrawal).
/// The total share balance of a holder is `unlocked + locked`.
#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShareBalance {
    pub unlocked: i128,
    pub locked: i128,
}

const SUPPORTED_ASSETS_KEY: Symbol = Symbol::short("assets");
const ASSET_BALANCES_KEY: Symbol = Symbol::short("balances");
const SHARE_BALANCES_KEY: Symbol = Symbol::short("shares");

fn supported_assets(env: &Env) -> Map<Address, AssetMetadata> {
    env.storage()
        .instance()
        .get(&SUPPORTED_ASSETS_KEY)
        .unwrap_or_else(|| Map::new(env))
}

fn asset_balances(env: &Env) -> Map<Address, AssetBalance> {
    env.storage()
        .instance()
        .get(&ASSET_BALANCES_KEY)
        .unwrap_or_else(|| Map::new(env))
}

fn share_balances(env: &Env) -> Map<Address, ShareBalance> {
    env.storage()
        .instance()
        .get(&SHARE_BALANCES_KEY)
        .unwrap_or_else(|| Map::new(env))
}

fn validate_asset_metadata(metadata: &AssetMetadata) -> Result<(), VaultError> {
    if metadata.decimals < MIN_ASSET_DECIMALS || metadata.decimals > MAX_ASSET_DECIMALS {
        return Err(VaultError::UnsupportedAssetDecimals);
    }
    Ok(())
}

/// Register a supported asset. Rejects unsupported decimal precision and
/// duplicate registration of the same token identity so accounting cannot be
/// split across two metadata entries for the same token contract.
pub fn add_supported_asset(
    env: &Env,
    metadata: AssetMetadata,
) -> Result<(), VaultError> {
    validate_asset_metadata(&metadata)?;

    let mut assets = supported_assets(env);
    if assets.contains_key(metadata.token.clone()) {
        return Err(VaultError::AssetAlreadySupported);
    }
    assets.set(metadata.token.clone(), metadata);
    env.storage().instance().set(&SUPPORTED_ASSETS_KEY, &assets);
    Ok(())
}

/// Update metadata for an already-registered asset. The token identity is
/// immutable; only symbol/decimals may change, and decimals must remain within
/// the supported range. Existing balances are re-tagged with the new decimals
/// without rescaling the stored raw amount.
pub fn update_supported_asset(
    env: &Env,
    metadata: AssetMetadata,
) -> Result<(), VaultError> {
    validate_asset_metadata(&metadata)?;

    let mut assets = supported_assets(env);
    if !assets.contains_key(metadata.token.clone()) {
        return Err(VaultError::AssetNotSupported);
    }
    assets.set(metadata.token.clone(), metadata.clone());
    env.storage().instance().set(&SUPPORTED_ASSETS_KEY, &assets);

    let mut balances = asset_balances(env);
    if let Some(existing) = balances.get(metadata.token.clone()) {
        balances.set(
            metadata.token.clone(),
            AssetBalance {
                amount: existing.amount,
                decimals: metadata.decimals,
            },
        );
        env.storage().instance().set(&ASSET_BALANCES_KEY, &balances);
    }
    Ok(())
}

pub fn is_supported_asset(env: &Env, token: &Address) -> bool {
    supported_assets(env).contains_key(token.clone())
}

pub fn get_supported_assets(env: &Env) -> Vec<AssetMetadata> {
    let assets = supported_assets(env);
    let mut out = Vec::new(env);
    for (_, metadata) in assets.iter() {
        out.push_back(metadata);
    }
    out
}

/// Record a per-asset balance in exact token units. The asset must be
/// registered so its declared decimals are known and stored alongside the
/// amount; no scaling is applied.
pub fn set_asset_balance(
    env: &Env,
    token: &Address,
    amount: i128,
) -> Result<(), VaultError> {
    let assets = supported_assets(env);
    let metadata = assets
        .get(token.clone())
        .ok_or(VaultError::AssetNotSupported)?;

    let mut balances = asset_balances(env);
    balances.set(
        token.clone(),
        AssetBalance {
            amount,
            decimals: metadata.decimals,
        },
    );
    env.storage().instance().set(&ASSET_BALANCES_KEY, &balances);
    Ok(())
}

pub fn get_asset_balance(env: &Env, token: &Address) -> Option<AssetBalance> {
    asset_balances(env).get(token.clone())
}

/// Read the split share balance for a holder. Missing entries are treated as
/// zero shares so callers can reason about locked/unlocked uniformly.
pub fn get_share_balance(env: &Env, holder: &Address) -> ShareBalance {
    share_balances(env)
        .get(holder.clone())
        .unwrap_or(ShareBalance {
            unlocked: 0,
            locked: 0,
        })
}

/// Persist the split share balance for a holder. Negative components are
/// rejected so accounting cannot be corrupted by a bad caller.
pub fn set_share_balance(
    env: &Env,
    holder: &Address,
    balance: ShareBalance,
) -> Result<(), VaultError> {
    if balance.unlocked < 0 || balance.locked < 0 {
        return Err(VaultError::InsufficientShares);
    }
    let mut balances = share_balances(env);
    balances.set(holder.clone(), balance);
    env.storage().instance().set(&SHARE_BALANCES_KEY, &balances);
    Ok(())
}

/// Total shares (locked + unlocked) held by a holder.
pub fn total_shares(env: &Env, holder: &Address) -> i128 {
    let balance = get_share_balance(env, holder);
    balance.unlocked + balance.locked
}

/// Shares that may be redeemed through a lock-respecting entrypoint
/// (`withdraw`, `withdraw_all`, asset withdrawal, migration).
pub fn unlocked_shares(env: &Env, holder: &Address) -> i128 {
    get_share_balance(env, holder).unlocked
}

/// Shares that are subject to a share lock and may only be released by an
/// entrypoint that explicitly documents a lock bypass (emergency withdrawal).
pub fn locked_shares(env: &Env, holder: &Address) -> i128 {
    get_share_balance(env, holder).locked
}

/// Deduct `amount` from the unlocked bucket only. Returns
/// `InsufficientShares` when the unlocked balance cannot cover the request,
/// even if locked shares would. This is the shared lock policy used by every
/// lock-respecting withdrawal entrypoint.
pub fn deduct_unlocked_shares(
    env: &Env,
    holder: &Address,
    amount: i128,
) -> Result<(), VaultError> {
    if amount < 0 {
        return Err(VaultError::InsufficientShares);
    }
    let mut balance = get_share_balance(env, holder);
    if balance.unlocked < amount {
        return Err(VaultError::InsufficientShares);
    }
    balance.unlocked -= amount;
    set_share_balance(env, holder, balance)
}

/// Deduct `amount` from the unlocked bucket first, then from the locked bucket.
/// This is the explicit emergency bypass: it is the only helper that may spend
/// locked shares, and it is intended solely for the emergency withdrawal path.
pub fn deduct_all_shares(
    env: &Env,
    holder: &Address,
    amount: i128,
) -> Result<(), VaultError> {
    if amount < 0 {
        return Err(VaultError::InsufficientShares);
    }
    let mut balance = get_share_balance(env, holder);
    let total = balance.unlocked + balance.locked;
    if total < amount {
        return Err(VaultError::InsufficientShares);
    }
    let from_unlocked = if balance.unlocked < amount {
        balance.unlocked
    } else {
        amount
    };
    balance.unlocked -= from_unlocked;
    balance.locked -= amount - from_unlocked;
    set_share_balance(env, holder, balance)
}
