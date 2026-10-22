use soroban_sdk::contracterror;

/// Errors returned by the NeuroWealth vault contract.
///
/// Asset metadata validation (issue #430) uses the `InvalidAssetDecimals` and
/// `DuplicateAsset` variants so callers can distinguish unsupported precision
/// from a token that has already been registered.
#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq, PartialOrd, Ord)]
#[repr(u32)]
pub enum VaultError {
    /// The caller is not authorized to perform this action.
    Unauthorized = 1,
    /// The vault has not been initialized.
    NotInitialized = 2,
    /// The vault has already been initialized.
    AlreadyInitialized = 3,
    /// The requested asset is not supported by the vault.
    UnsupportedAsset = 4,
    /// The asset is already registered; duplicate registration is rejected so
    /// token accounting cannot be split across two entries.
    DuplicateAsset = 5,
    /// The asset declares a decimal precision outside the supported range.
    InvalidAssetDecimals = 6,
    /// The asset symbol is empty or otherwise invalid.
    InvalidAssetSymbol = 7,
    /// An amount is invalid (zero, negative, or overflows when scaled).
    InvalidAmount = 8,
    /// Arithmetic overflow while scaling or accumulating balances.
    Overflow = 9,
    /// The vault is paused.
    Paused = 10,
}
