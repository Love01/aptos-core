module token_v2::coin {
    use aptos_std::smart_table::{Self, SmartTable};
    use aptos_framework::object::{Object, object_address, create_object_from_account, ungated_transfer_allowed, is_owner, create_object_from_object};
    use std::signer;
    use aptos_framework::object;
    use token_v2::common::{increase_supply, refs_contain_transfer, new_root_cap, Refs, fungible_asset_metadata_exists, get_asset_owner_cap_flags};
    use std::signer::address_of;
    use std::vector;
    use token_v2::common;

    struct CoinStore<phantom T> has key {
        index: SmartTable<Object<T>, Object<Coin<T>>>
    }

    struct Coin<phantom T> has key {
        asset: Object<T>,
        balance: u64,
    }

    struct CashedCoin<phantom T> {
        asset: Object<T>,
        amount: u64,
    }

    struct CoinRefs<phantom T> has key {
        refs: Refs<Coin<T>>
    }

    /// ================================================================================================================
    /// Public functions
    /// ================================================================================================================

    /// Ensure the coin store exists. If not, create it.
    public fun ensure_coin_store<T>(creator: &signer) {
        if (!exists<CoinStore<T>>(signer::address_of(creator))) {
            move_to(creator, CoinStore {
                index: smart_table::new()
            })
        }
    }

    /// Merge a vector of CashedCoins into one.
    public fun merge_cash<T>(cash: vector<CashedCoin<T>>): CashedCoin<T> {
        assert!(vector::length(&cash) > 0);
        let asset = vector::borrow(&cash, 0).asset;
        vector::fold(CashedCoin {
            asset,
            amount: 0
        }, | c1, c2 | { merge_cash_internal(c1, c2) })
    }

    /// Assert the object is an eligible asset to issue fungible tokens.
    public inline fun assert_fungible_asset_enabled<T>(asset: Object<T>) {
        assert!(fungible_asset_metadata_exists(&asset), 0);
    }

    /// Mint fungible tokens as the owner of the base asset.
    public fun mint<T>(asset_owner: &signer,  asset: Object<T>, amount: u64): CashedCoin<T> {
        // assert the asset has enabled fungible tokens.
        assert_fungible_asset_enabled(asset);
        assert!(object::is_owner(asset, signer::address_of(asset_owner)));
        increase_supply(object_address(&asset), amount);
        mint_cash(asset, amount)
    }


    public fun create_coin_from_asset<T: key>(account: &signer, asset: Object<T>): Object<Coin<T>> {
        assert_fungible_asset_enabled(asset);
        let creator_ref = create_object_from_account(account);
        let coin_signer = object::generate_signer(&creator_ref);
        let root_cap = new_root_cap(creator_ref);
        let asset_owner_cap_flags = get_asset_owner_cap_flags(object_address(&asset));
        let refs = common::new_refs_from_constructor_ref<T>(&root_cap, asset_owner_cap_flags);
        move_to(&coin_signer, Coin {
            asset,
            balance: 0
        });
        move_to(&coin_signer, CoinRefs<T>{ refs });
        object::object_from_constructor_ref<Coin<T>>(&creator_ref)
    }

    public fun coin_freeze<T>(creator: &signer, owner: address, asset: Object<T>) acquires Coin, CoinStore, CoinRefs {
        // Those may not exist.
        assert!(exists<CoinStore<T>>(owner), 0);
        let index_table = &mut borrow_global_mut<CoinStore<T>>(owner).index;
        // Those may not exist.
        assert!(smart_table::contains(index_table, asset), 0);
        let coin_obj = *smart_table::borrow(index_table, asset);
        assert!(exists<Coin<T>>(object_address(&coin_obj)), 0);
        let coin = borrow_global<Coin<T>>(object_address(&coin_obj));
        // assert creator is the owner of the coin asset.
        assert!(is_owner(coin.asset, address_of(creator)), 0);
        let refs = &borrow_global<CoinRefs<T>>(object_address(&coin_obj)).refs;
        assert!(refs_contain_transfer(refs), 0);
        object::disable_ungated_transfer(common::borrow_transfer_ref_from_cap(common::borrow_transfer_from_refs(refs)));
    }

    public fun coin_unfreeze<T>(creator: &signer, owner: address, asset: Object<T>) acquires Coin, CoinStore, CoinRefs {
        // Those may not exist.
        if (!exists<CoinStore<T>>(owner)) {
            return;
        };
        let index_table = &mut borrow_global_mut<CoinStore<T>>(owner).index;
        // Those may not exist.
        if (!smart_table::contains(index_table, asset)) {
            return;
        };
        let coin_obj = *smart_table::borrow(index_table, asset);
        assert!(exists<Coin<T>>(object_address(&coin_obj)), 0);
        let coin = borrow_global<Coin<T>>(object_address(&coin_obj));
        // assert creator is the owner of the coin asset.
        assert!(is_owner(coin.asset, address_of(creator)), 0);
        let refs = &borrow_global<CoinRefs<T>>(object_address(&coin_obj)).refs;
        assert!(refs_contain_transfer(refs), 0);
        object::enable_ungated_transfer(common::borrow_transfer_ref_from_cap(common::borrow_transfer_from_refs(refs)));
    }

    public fun is_coin_frozen<T>(owner: address, asset: Object<T>): bool acquires Coin, CoinStore {
        // Those may not exist.
        if (!exists<CoinStore<T>>(owner)) {
            return false;
        };
        let index_table = &mut borrow_global_mut<CoinStore<T>>(owner).index;
        // Those may not exist.
        if (!smart_table::contains(index_table, asset)) {
            return false;
        };
        let coin_obj = *smart_table::borrow(index_table, asset);
        assert!(exists<Coin<T>>(object_address(&coin_obj)), 0);
        ungated_transfer_allowed(coin_obj)
    }

    public fun withdraw<T: key>(account: &signer, asset: Object<T>, amount: u64): CashedCoin<T> acquires CoinStore, Coin, CoinRefs {
        assert!(amount > 0, 0);
        let addr = signer::address_of(account);
        assert!(exists<CoinStore<T>>(addr), 0);
        let sender_index_table = &mut borrow_global_mut<CoinStore<T>>(addr).index;
        // sender must have the coin balance
        assert!(smart_table::contains(sender_index_table, asset), 0);
        let coin_obj = *smart_table::borrow(sender_index_table, asset);
        // ensure the owner.
        assert!(is_owner(coin_obj, addr), 0);
        // ensure allow transfer, which includes partial transfer in the context of fungible asset.
        assert!(ungated_transfer_allowed(coin_obj), 0);
        let coin = borrow_global_mut<Coin<T>>(object_address(&coin_obj));
        let cash = withdraw_cash(coin, amount);
        remove_zero_coin_if_enabled(sender_index_table, coin_obj);
        cash
    }

    public fun deposit<T: key>(account: &signer, to: address, asset: Object<T>, cash: CashedCoin<T>) acquires CoinStore, Coin {
        assert!(exists<CoinStore<T>>(to), 0);
        // add balance
        let index_table = &mut borrow_global_mut<CoinStore<T>>(to).index;
        if (!smart_table::contains(index_table, asset)) {
            let coin_obj = create_coin_from_asset(account, asset);
            object::transfer(account, coin_obj, to);
            smart_table::add(index_table, asset, coin_obj);
        };
        let coin_obj = *smart_table::borrow(index_table, asset);
        // ensure allow transfer no matter it is NFT or fungible asset.
        assert!(ungated_transfer_allowed(coin_obj), 0);
        let coin = borrow_global_mut<Coin<T>>(object_address(&coin_obj));
        // ensure coin object has enough balance.
        deposit_cash(coin, cash);
    }

    public fun transfer<T: key>(account: &signer, receiver: address, asset: Object<T>, amount: u64) acquires CoinStore, Coin, CoinRefs {
        assert!(amount > 0, 0);
        let cash = withdraw(account, asset, amount);
        deposit(account, receiver, asset, cash);
    }

    /// ================================================================================================================
    /// Private functions
    /// ================================================================================================================
    fun mint_cash<T>(asset: Object<T>, amount: u64): CashedCoin<T> {
        assert!(amount > 0, 0);
        CashedCoin<T> {
            asset,
            amount
        }
    }

    fun withdraw_cash<T>(coin: &mut Coin<T>, amount: u64): CashedCoin<T> {
        assert!(coin.balance >= amount, 0);
        coin.balance = coin.balance - amount;
        CashedCoin {
            asset: coin.asset,
            amount
        }
    }

    fun merge_cash_internal<T>(cash1: CashedCoin<T>, cash2: CashedCoin<T>): CashedCoin<T> {
        let CashedCoin {
            asset,
            amount: amount1
        } = cash1;
        let CashedCoin {
            asset: asset2,
            amount: amount2,
        } = cash2;
        // Make sure the cash is for the same asset.
        assert!(asset == asset2);
        CashedCoin {
            asset,
            amount: amount1 + amount2
        }
    }

    fun deposit_cash<T>(coin: &mut Coin<T>, cash: CashedCoin<T>) {
        // ensure merging the same coin
        let CashedCoin { asset, amount } = cash;
        assert!(coin.asset == asset, 0);
        coin.balance = coin.balance + amount;
    }

    fun remove_zero_coin_if_enabled<T>(index: &mut SmartTable<Object<T>, Object<Coin<T>>>, coin_obj: Object<Coin<T>>) acquires Coin, CoinRefs {
        let coin_address = object_address(&coin_obj);
        let coin = borrow_global<Coin<T>>(coin_address);
        if (borrow_global<Coin<T>>(object_address(&coin_obj)).balance == 0 && common::refs_contain_delete(&borrow_global<CoinRefs<T>>(coin_address).refs)) {
            smart_table::remove(index, coin.asset);
            move_from<Coin<T>>(coin_address);
            let refs = move_from<CoinRefs<T>>(coin_address).refs;
            object::delete(common::extract_delete_ref_from_cap(common::extract_delete_from_refs(&mut refs)));
        }
    }
}
