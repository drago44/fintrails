use serde::{Deserialize, Serialize};

use crate::account::{AccountId, Asset};

/// One leg of a transaction: a signed change of one asset on one account.
///
/// `amount` is in minor units. Sign carries direction: positive flows into
/// the account, negative flows out. Balancing (`Σ == 0`) is a property of the
/// whole transaction, not of a single posting.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Posting {
    pub account: AccountId,
    pub asset: Asset,
    pub amount: i128,
}
