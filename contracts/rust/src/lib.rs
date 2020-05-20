use borsh::{BorshDeserialize, BorshSerialize};
use near_sdk::collections::Map;
use near_sdk::collections::Set;
use near_sdk::{env, near_bindgen, AccountId};

#[global_allocator]
static ALLOC: wee_alloc::WeeAlloc = wee_alloc::WeeAlloc::INIT;

/// This trait provides the baseline of functions as described at:
/// https://github.com/nearprotocol/NEPs/blob/nep-4/specs/Standards/Tokens/NonFungibleToken.md
pub trait NEP4 {
    // Grant the access to the given `accountId` for the given `tokenId`.
    // Requirements:
    // * The caller of the function (`predecessor_id`) should have access to the token.
    fn grant_access(&mut self, escrow_account_id: AccountId);

    // Revoke the access to the given `accountId` for the given `tokenId`.
    // Requirements:
    // * The caller of the function (`predecessor_id`) should have access to the token.
    fn revoke_access(&mut self, escrow_account_id: AccountId);

    // Transfer the given `tokenId` to the given `accountId`.  Account `accountId` becomes the new owner.
    // Requirements:
    // * The caller of the function (`predecessor_id`) should have access to the token.
    fn transfer(&mut self, new_owner_id: AccountId, token_id: TokenId);

    // Returns `true` or `false` based on caller of the function (`predecessor_id) having access to the token
    fn check_access(&self, account_id: AccountId) -> bool;

    // Get an individual owner by given `tokenId`.
    fn get_token_owner(&self, token_id: TokenId) -> String;
}

/// The token ID type is also defined in the NEP
pub type TokenId = u64;
pub type AccountIdHash = Vec<u8>;

// Begin implementation
#[near_bindgen]
#[derive(BorshDeserialize, BorshSerialize)]
pub struct NonFungibleTokenBasic {
    pub token_to_account: Map<TokenId, AccountId>,
    pub account_to_set: Map<AccountId, Set<TokenId>>, // instead of AccountId Vec<u8>?
    pub account_gives_access: Map<AccountIdHash, Set<AccountIdHash>>, // Vec<u8> is sha256 of account, makes it safer and is how fungible token also works
    pub owner_id: AccountId,
}

impl Default for NonFungibleTokenBasic {
    fn default() -> Self {
        panic!("Fun token should be initialized before usage")
    }
}

#[near_bindgen]
impl NonFungibleTokenBasic {
    #[init]
    pub fn new(owner_id: AccountId) -> Self {
        assert!(env::is_valid_account_id(owner_id.as_bytes()), "Owner's account ID is invalid.");
        assert!(!env::state_exists(), "Already initialized");
        Self {
            token_to_account: Map::new(b"token-belongs-to".to_vec()),
            account_to_set: Map::new(b"account-has-set".to_vec()),
            account_gives_access: Map::new(b"gives-access".to_vec()),
            owner_id,
        }
    }
}

impl NEP4 for NonFungibleTokenBasic {
    fn grant_access(&mut self, escrow_account_id: AccountId) {
        let escrow_hash = env::sha256(escrow_account_id.as_bytes());
        let signer = env::signer_account_id();
        let signer_hash = env::sha256(signer.as_bytes());

        let mut access_set = match self.account_gives_access.get(&signer_hash) {
            Some(existing_set) => {
                existing_set
            },
            None => {
                Set::new(b"new-access-set".to_vec())
            }
        };
        access_set.insert(&escrow_hash);
        self.account_gives_access.insert(&signer_hash, &access_set);
    }

    fn revoke_access(&mut self, escrow_account_id: AccountId) {
        let signer = env::signer_account_id();
        let signer_hash = env::sha256(signer.as_bytes());
        let mut existing_set = match self.account_gives_access.get(&signer_hash) {
            Some(existing_set) => existing_set,
            None => env::panic(b"Access does not exist.")
        };
        let escrow_hash = env::sha256(escrow_account_id.as_bytes());
        if existing_set.contains(&escrow_hash) {
            existing_set.remove(&escrow_hash);
            self.account_gives_access.insert(&signer_hash, &existing_set);
            env::log(b"Successfully removed access.")
        } else {
            env::panic(b"Did not find access for escrow ID.")
        }
    }

    fn transfer(&mut self, new_owner_id: AccountId, token_id: TokenId) {
        let token_owner_account_id = self.get_token_owner(token_id);
        if !self.check_access(token_owner_account_id) {
            env::panic(b"Attempt to transfer a token with no access.")
        }
        // TODO: remove this. This is a temporary workaround until the underlying root cause is addressed.
        workaround();
        self.token_to_account.insert(&token_id, &new_owner_id);
    }

    fn check_access(&self, account_id: AccountId) -> bool {
        let account_hash = env::sha256(account_id.as_bytes());
        match self.account_gives_access.get(&account_hash) {
            Some(access) => {
                let signer = env::signer_account_id();
                let signer_hash = env::sha256(signer.as_bytes());
                access.contains(&signer_hash)
            },
            None => false
        }
    }

    fn get_token_owner(&self, token_id: TokenId) -> String {
        match self.token_to_account.get(&token_id) {
            Some(owner_id) => owner_id,
            None => env::panic(b"No owner of the token ID specified")
        }
    }
}

/// Methods not in the strict scope of the NFT spec (NEP4)
#[near_bindgen]
impl NonFungibleTokenBasic {
    /// Creates a token for owner_id, doesn't use autoincrement, fails if id is taken
    pub fn mint_token(&mut self, owner_id: String, token_id: TokenId) {
        // make sure that only the owner can call this funtion
        self.only_owner();
        // Since Map doesn't have `contains` we use match
        let token_check = self.token_to_account.get(&token_id);
        if token_check.is_some() {
            env::panic(b"Token ID already exists.")
        }
        // No token with that ID exists, mint and add token to data structures
        self.token_to_account.insert(&token_id, &owner_id);
        // Add to account_to_set
        let mut token_set = match self.account_to_set.get(&owner_id) {
            Some(existing_set) => {
                existing_set
            },
            None => {
                Set::new(b"new-access-set".to_vec())
            }
        };
        token_set.insert(&token_id);
        self.account_to_set.insert(&owner_id, &token_set);
    }

    /// helper function determining contract ownership
    fn only_owner(&mut self) {
        assert_eq!(env::signer_account_id(), self.owner_id, "Only contract owner can call this method.");
    }
}

// This is a workaround for https://github.com/near/near-sdk-rs/issues/159
// This is a very temporary solution until this issue is fixed. We apologize for the
// temporary need to have this.
fn workaround() {
    // copy pasted basic usage from tests
    // https://github.com/near/near-sdk-rs/blob/master/near-sdk/src/collections/map.rs
    // https://github.com/near/near-sdk-rs/blob/master/near-sdk/src/collections/set.rs
    let mut map: Map<u64, u64> = Map::default();
    let key1 = 1u64;
    let value1 = 2u64;
    map.insert(&key1, &value1);

    let mut set: Set<u64> = Set::default();
    let key1 = 1u64;
    set.insert(&key1);
}

// use the attribute below for unit tests
#[cfg(test)]
mod tests {
    use super::*;
    use near_sdk::MockedBlockchain;
    use near_sdk::{testing_env, VMContext};

    // part of writing unit tests is setting up a mock context
    // in this example, this is only needed for env::log in the contract
    // this is also a useful list to peek at when wondering what's available in env::*
    fn get_context(signer_account_id: String) -> VMContext {
        VMContext {
            current_account_id: "alice.testnet".to_string(),
            signer_account_id: signer_account_id,
            signer_account_pk: vec![0, 1, 2],
            predecessor_account_id: "jane.testnet".to_string(),
            input: vec![],
            block_index: 0,
            block_timestamp: 0,
            account_balance: 0,
            account_locked_balance: 0,
            storage_usage: 0,
            attached_deposit: 0,
            prepaid_gas: 10u64.pow(18),
            random_seed: vec![0, 1, 2],
            is_view: false,
            output_data_receivers: vec![],
            epoch_height: 19,
        }
    }

    #[test]
    fn grant_access() {
        let context = get_context("robert.testnet".to_string());
        testing_env!(context);
        let mut contract = NonFungibleTokenBasic::new("robert.testnet".to_string());
        let length_before = contract.account_gives_access.len();
        assert_eq!(0, length_before, "Expected empty account access Map.");
        contract.grant_access("mike.testnet".to_string());
        contract.grant_access("kevin.testnet".to_string());
        let length_after = contract.account_gives_access.len();
        assert_eq!(1, length_after, "Expected an entry in the account's access Map.");
        let signer_hash = env::sha256("robert.testnet".as_bytes());
        let num_grantees = contract.account_gives_access.get(&signer_hash).unwrap();
        assert_eq!(2, num_grantees.len(), "Expected two accounts to have access to signer.");
    }

    #[test]
    #[should_panic(
        expected = r#"Access does not exist."#
    )]
    fn revoke_access_and_panic() {
        let context = get_context("robert.testnet".to_string());
        testing_env!(context);
        let mut contract = NonFungibleTokenBasic::new("robert.testnet".to_string());
        contract.revoke_access("kevin.testnet".to_string());
    }

    #[test]
    fn add_revoke_access_and_check() {
        let context = get_context("robert.testnet".to_string());
        testing_env!(context);
        let mut contract = NonFungibleTokenBasic::new("robert.testnet".to_string());
        contract.grant_access("robert.testnet".to_string());
        let mut robert_has_access = contract.check_access("robert.testnet".to_string());
        assert_eq!(true, robert_has_access, "After granting access, check_access call failed.");
        contract.revoke_access("robert.testnet".to_string());
        robert_has_access = contract.check_access("robert.testnet".to_string());
        assert_eq!(false, robert_has_access, "After revoking access, check_access call failed.");
    }

    #[test]
    fn mint_token_get_token_owner() {
        let context = get_context("robert.testnet".to_string());
        testing_env!(context);
        let mut contract = NonFungibleTokenBasic::new("robert.testnet".to_string());
        contract.mint_token("mike.testnet".to_string(), 19u64);
        let owner = contract.get_token_owner(19u64);
        assert_eq!("mike.testnet".to_string(), owner, "Unexpected token owner.");
    }

    #[test]
    #[should_panic(
        expected = r#"Attempt to transfer a token with no access."#
    )]
    fn transfer_with_no_access_should_fail() {
        // Mike owns the token.
        // Robert is trying to transfer it to Robert's account without having access.
        let context = get_context("robert.testnet".to_string());
        testing_env!(context);
        let mut contract = NonFungibleTokenBasic::new("robert.testnet".to_string());
        let token_id = 19u64;
        contract.mint_token("mike.testnet".to_string(), token_id);

        contract.transfer("robert.testnet".to_string(), token_id);
    }

    #[test]
    fn transfer_with_escrow_access() {
        // Escrow account: robert.testnet
        // Owner account: mike.testnet
        // New owner account: joe.testnet

        testing_env!(get_context("mike.testnet".to_string()));
        let mut contract = NonFungibleTokenBasic::new("mike.testnet".to_string());
        let token_id = 19u64;
        contract.mint_token("mike.testnet".to_string(), token_id);
        // Mike grants access to Robert
        contract.grant_access("robert.testnet".to_string());

        // Robert transfers the token to Joe
        // TODO: figure out how to test
        testing_env!(get_context("robert.testnet".to_string()));
        contract.transfer("joe.testnet".to_string(), token_id);

        // Check new owner
        let owner = contract.get_token_owner(token_id);
        assert_eq!("joe.testnet".to_string(), owner, "Token was not transferred after transfer call with escrow.");
    }

    #[test]
    fn transfer_with_your_own_token() {
        // Owner account: robert.testnet
        // New owner account: joe.testnet

        testing_env!(get_context("robert.testnet".to_string()));
        let mut contract = NonFungibleTokenBasic::new("robert.testnet".to_string());
        let token_id = 19u64;
        contract.mint_token("robert.testnet".to_string(), token_id);
        // workaround until we can add self-check in check-access
        // TODO: remove this line
        contract.grant_access("robert.testnet".to_string());

        // Robert transfers the token to Joe
        contract.transfer("joe.testnet".to_string(), token_id);

        // Check new owner
        let owner = contract.get_token_owner(token_id);
        assert_eq!("joe.testnet".to_string(), owner, "Token was not transferred after transfer call with escrow.");
    }


    // #[test]
    // #[should_panic(
    //     expected = r#"No access entries for this account."#
    // )]
    // good next test
}