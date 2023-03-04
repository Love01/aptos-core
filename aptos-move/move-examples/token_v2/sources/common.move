module token_v2::common {
    use std::option::{Option, is_some};
    use aptos_framework::object::{Self, ExtendRef, TransferRef, DeleteRef, Object, ConstructorRef, object_from_extend_ref, object_from_transfer_ref, object_from_delete_ref, object_address, transfer_with_ref, generate_linear_transfer_ref, enable_ungated_transfer, disable_ungated_transfer, generate_extend_ref, generate_transfer_ref, generate_delete_ref, exists_at, is_owner};
    use std::option;
    use token_v2::collection::Collection;
    use std::string::{String, bytes};
    use std::vector;
    use std::string;
    use std::error;
    use std::signer::address_of;

    friend token_v2::collection;
    friend token_v2::coin;
    friend token_v2::token;

    /// The length of ref_flags vector is not 3.
    const EREF_FLAGS_INCORRECT_LENGTH: u64 = 1;
    /// Object<T> (Resource T) does not exist.
    const EOBJECT_NOT_FOUND: u64 = 2;
    /// Not the owner.
    const ENOT_OWNER: u64 = 3;
    /// The fungible asset metadata does not exist for this asset object.
    const EFUNGIBLE_ASSET_METADATA_NOT_FOUND: u64 = 4;
    /// ExtendRef exists or does not exist.
    const EEXTEND_REF_EXISTENCE: u64 = 5;
    /// TransferRef exists does not exist.
    const ETRANSFER_REF_EXISTENCE: u64 = 6;
    /// DeleteRef exists or does not exist.
    const EDELETE_REF_EXISTENCE: u64 = 7;
    /// Royalty percentage is invalid.
    const EINVALID_PERCENTAGE: u64 = 8;
    /// Name is invalid.
    const EINVALID_NAME: u64 = 9;

    public fun assert_ref_flags_length(flags: &vector<bool>) {
        assert!(vector::length(flags) == 3, error::invalid_argument(EREF_FLAGS_INCORRECT_LENGTH));
    }

    public fun assert_valid_name(name: &String) {
        assert!(is_valid_name(name), error::invalid_argument(EINVALID_NAME));
    }

    /// Only allow human readable characters in naming.
    fun is_valid_name(name: &String):bool {
        if (string::length(name) == 0) {
            return false;
        };
        std::vector::all(bytes(name), |char| *char >= 32 && *char <= 126)
    }

    /// ================================================================================================================
    /// Royalty
    /// ================================================================================================================
    #[resource_group_member(group = aptos_framework::object::ObjectGroup)]
    /// The royalty of a token within this collection -- this optional
    struct Royalty has copy, drop, key {
        // The percentage of sale price considered as royalty.
        percentage: u8,
        /// The recipient of royalty payments. See the `shared_account` for how to handle multiple
        /// creators.
        payee_address: address,
    }

    public(friend) fun create_royalty(percentage: u8, payee_address: address): Royalty {
        assert!(percentage <= 100, error::invalid_argument(EINVALID_PERCENTAGE));
        Royalty { percentage, payee_address }
    }

    public(friend) fun init_royalty(object_signer: &signer, royalty: Royalty) {
        move_to(object_signer, royalty);
    }

    public(friend) fun remove_royalty(object_address: address) acquires Royalty {
        move_from<Royalty>(object_address);
    }

    public(friend) fun exists_royalty(object_address: address): bool {
        exists<Royalty>(object_address)
    }

    public fun get_royalty(object_addr: address): Option<Royalty> acquires Royalty {
        if (exists<Royalty>(object_addr)) {
            option::some(*borrow_global<Royalty>(object_addr))
        } else {
            option::none()
        }
    }

    public fun get_royalty_pencentage(royalty: &Royalty): u8 {
        royalty.percentage
    }

    public fun get_royalty_payee_address(royalty: &Royalty): address {
        royalty.payee_address
    }


    /// ================================================================================================================
    /// Fungible asset metadata
    /// ================================================================================================================
    #[resource_group_member(group = aptos_framework::object::ObjectGroup)]
    struct FungibleAssetMetadata has copy, drop, key {
        current_supply: u64,
        asset_owner_ref_flags: vector<bool>,
    }

    public fun init_fungible_asset_data(object_signer: &signer, asset_owner_ref_flags: vector<bool>) {
        assert_ref_flags_length(&asset_owner_ref_flags);
        move_to(object_signer,
            FungibleAssetMetadata {
                current_supply: 0,
                asset_owner_ref_flags
            }
        );
    }

    public fun assert_fungible_asset_metadata_exists<T>(asset: Object<T>) {
        assert!(fungible_asset_metadata_exists(asset), error::not_found(EFUNGIBLE_ASSET_METADATA_NOT_FOUND));
    }

    public fun fungible_asset_metadata_exists<T>(asset: &Object<T>):bool {
        exists<FungibleAssetMetadata>(object_address(asset))
    }

    public fun remove_fungible_asset_metadata<T>(owner: &signer, asset: Object<T>) acquires FungibleAssetMetadata {
        assert!(is_owner(asset, address_of(owner)), error::permission_denied(ENOT_OWNER));
        move_from<FungibleAssetMetadata>(object_address(&asset));
    }

    public fun get_current_supply<T>(asset: Object<T>): u64 acquires FungibleAssetMetadata {
        assert_fungible_asset_metadata_exists(asset);
        let object_addr = object_address(&asset);
        *borrow_global<FungibleAssetMetadata>(object_addr).current_supply
    }

    public fun get_asset_owner_ref_flags<T>(asset: Object<T>): vector<bool> acquires FungibleAssetMetadata {
        let object_addr = object_address(&asset);
        assert_fungible_asset_metadata_exists(asset);
        *borrow_global<FungibleAssetMetadata>(object_addr).asset_owner_ref_flags
    }

    public fun increase_supply<T>(owner: &signer, asset: Object<T>, amount: u64) acquires FungibleAssetMetadata {
        assert_fungible_asset_metadata_exists(asset);
        assert!(is_owner(asset, address_of(owner)), error::permission_denied(ENOT_OWNER));
        let object_addr = object_address(&asset);
        let supply = &mut borrow_global_mut<FungibleAssetMetadata>(object_addr).current_supply;
        *supply = *supply + amount;
    }

    public fun decrease_supply<T>(owner: &signer, asset: Object<T>, amount: u64) acquires FungibleAssetMetadata {
        assert_fungible_asset_metadata_exists(asset);
        assert!(is_owner(asset, address_of(owner)), error::permission_denied(ENOT_OWNER));
        let object_addr = object_address(&asset);
        let supply = &mut borrow_global_mut<FungibleAssetMetadata>(object_addr).current_supply;
        *supply = *supply - amount;
    }

    /// ================================================================================================================
    /// Refs - a collection of ExtendRef, TransferRef and DeleteRef.
    /// ================================================================================================================
    struct Refs<phantom T> has drop, store {
        object: Object<T>,
        extend: Option<ExtendRef>,
        transfer: Option<TransferRef>,
        delete: Option<DeleteRef>,
    }

    public fun new_refs<T>(object: Object<T>): Refs<T> {
        Refs {
            object,
            extend: option::none<ExtendRef>(),
            transfer: option::none<TransferRef>(),
            delete: option::none<DeleteRef>(),
        }
    }

    public(friend) fun new_refs_from_constructor_ref<T: key>(constructor_ref: &ConstructorRef, enabled_refs: vector<bool>): Refs<T> {
        assert_ref_flags_length(&enabled_refs);
        let enable_extend = *vector::borrow(&enabled_refs, 0);
        let enable_transfer = *vector::borrow(&enabled_refs, 1);
        let enable_delete = *vector::borrow(&enabled_refs, 2);
        Refs {
            object: object::object_from_constructor_ref<T>(constructor_ref),
            extend: if (enable_extend) {option::some(generate_extend_ref(constructor_ref))} else {option::none()},
            transfer: if (enable_transfer) {option::some(generate_transfer_ref(constructor_ref))} else {option::none()},
            delete: if (enable_delete) {option::some(generate_delete_ref(constructor_ref))} else {option::none()},
        }
    }

    public fun get_object_from_refs<T>(refs: &Refs<T>): Object<T> {
        refs.object
    }

    public fun add_extend_to_refs<T: key>(refs: &mut Refs<T>, ref: ExtendRef) {
        assert!(option::is_none(&refs.extend), error::already_exists(EEXTEND_REF_EXISTENCE));
        assert!(&object_from_extend_ref<T>(&ref) == &refs.object);
        option::fill(&mut refs.extend, ref);
    }

    public fun add_transfer_to_refs<T: key>(refs: &mut Refs<T>, ref: TransferRef) {
        assert!(option::is_none(&refs.transfer), error::already_exists(ETRANSFER_REF_EXISTENCE));
        assert!(&object_from_transfer_ref<T>(&ref) == &refs.object);
        option::fill(&mut refs.transfer, ref);
    }

    public fun add_delete_to_refs<T: key>(refs: &mut Refs<T>, ref: DeleteRef) {
        assert!(option::is_none(&refs.delete), error::already_exists(EDELETE_REF_EXISTENCE));
        assert!(&object_from_delete_ref<T>(&ref) == &refs.object);
        option::fill(&mut refs.delete, ref);
    }

    public fun refs_contain_extend<T>(refs: &Refs<T>):bool {
        is_some(&refs.extend)
    }

    public fun refs_contain_transfer<T>(refs: &Refs<T>):bool {
        is_some(&refs.transfer)
    }

    public fun refs_contain_delete<T>(refs: &Refs<T>):bool {
        is_some(&refs.delete)
    }

    public fun borrow_extend_from_refs<T>(refs: &Refs<T>): &ExtendRef {
        assert!(is_some(&refs.extend), error::not_found(EEXTEND_REF_EXISTENCE));
        option::borrow(&refs.extend)
    }

    public fun borrow_transfer_from_refs<T>(refs: &Refs<T>): &TransferRef {
        assert!(is_some(&refs.transfer), error::not_found(ETRANSFER_REF_EXISTENCE));
        option::borrow(&refs.transfer)
    }

    public fun borrow_delete_from_refs<T>(refs: &Refs<T>): &DeleteRef {
        assert!(is_some(&refs.delete), error::not_found(EDELETE_REF_EXISTENCE));
        option::borrow(&refs.delete)
    }

    public fun extract_extend_from_refs<T>(refs: &mut Refs<T>): ExtendRef {
        assert!(is_some(&refs.extend), error::not_found(EEXTEND_REF_EXISTENCE));
        option::extract(&mut refs.extend)
    }

    public fun extract_transfer_from_refs<T>(refs: &mut Refs<T>): TransferRef {
        assert!(is_some(&refs.transfer), error::not_found(ETRANSFER_REF_EXISTENCE));
        option::extract(&mut refs.transfer)
    }

    public fun extract_delete_from_refs<T>(refs: &mut Refs<T>): DeleteRef {
        assert!(is_some(&refs.delete), error::not_found(EDELETE_REF_EXISTENCE));
        option::extract(&mut refs.delete)
    }
}
