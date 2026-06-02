use serde::{Deserialize, Serialize};

/// Account identifier, e.g. "merchant:payable".
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AccountId(pub String);

/// Asset / currency code, e.g. "USDC". Balancing is always per asset.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Asset(pub String);
