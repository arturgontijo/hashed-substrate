#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

mod functions;
mod types;

#[frame_support::pallet]
pub mod pallet {
	use frame_support::{pallet_prelude::{*, OptionQuery}, transactional};
	use frame_system::pallet_prelude::*;
	use sp_runtime::sp_std::vec::Vec;
	use crate::types::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type RemoveOrigin: EnsureOrigin<Self::Origin>;
		
		#[pallet::constant]
		type MaxAuthsPerMarket: Get<u32>;
		#[pallet::constant]
		type MaxRolesPerAuth: Get<u32>;
		#[pallet::constant]
		type MaxApplicants: Get<u32>;
		#[pallet::constant]
		type LabelMaxLen: Get<u32>;
		#[pallet::constant]
		type NotesMaxLen: Get<u32>;
		#[pallet::constant]
		type NameMaxLen: Get<u32>;
		#[pallet::constant]
		type MaxFiles: Get<u32>;
		#[pallet::constant]
		type MaxApplicationsPerCustodian: Get<u32>;
	}

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	/*--- Onchain storage section ---*/

	#[pallet::storage]
	#[pallet::getter(fn marketplaces)]
	pub(super) type Marketplaces<T: Config> = StorageMap<
		_, 
		Identity, 
		[u8; 32], 
		Marketplace<T>,
		OptionQuery,
	>;

	#[pallet::storage]
	#[pallet::getter(fn marketplaces_by_authority)]
	pub(super) type MarketplacesByAuthority<T: Config> = StorageDoubleMap<
		_, 
		Blake2_128Concat, 
		T::AccountId, // K1: Authority 
		Blake2_128Concat, 
		[u8;32], // K2: marketplace_id 
		BoundedVec<MarketplaceAuthority, T::MaxRolesPerAuth >, // scales with MarketplaceAuthority cardinality
		ValueQuery
	>;

	#[pallet::storage]
	#[pallet::getter(fn authorities_by_marketplace)]
	pub(super) type AuthoritiesByMarketplace<T: Config> = StorageDoubleMap<
		_, 
		Identity, 
		[u8;32], //K1: marketplace_id 
		Blake2_128Concat, 
		MarketplaceAuthority, //k2: authority
		BoundedVec<T::AccountId,T::MaxAuthsPerMarket>, 
		ValueQuery
	>;

	#[pallet::storage]
	#[pallet::getter(fn applications)]
	pub(super) type Applications<T: Config> = StorageMap<
		_, 
		Identity, 
		[u8;32], 
		Application<T>, 
		OptionQuery
	>;

	#[pallet::storage]
	#[pallet::getter(fn applications_by_account)]
	pub(super) type ApplicationsByAccount<T: Config> = StorageDoubleMap<
		_, 
		Blake2_128Concat, 
		T::AccountId, 
		Blake2_128Concat, 
		[u8;32], //marketplace_id 
		[u8;32], //application_id
		OptionQuery
	>;


	#[pallet::storage]
	#[pallet::getter(fn applicants_by_marketplace)]
	pub(super) type ApplicantsByMarketplace<T: Config> = StorageDoubleMap<
		_, 
		Identity, 
		[u8;32], 
		Blake2_128Concat, 
		ApplicationStatus, 
		BoundedVec<T::AccountId,T::MaxApplicants>, 
		ValueQuery
	>;

	#[pallet::storage]
	#[pallet::getter(fn custodians)]
	pub(super) type Custodians<T: Config> = StorageDoubleMap<
		_, 
		Blake2_128Concat, 
		T::AccountId, //custodians
		Blake2_128Concat, 
		[u8;32], //marketplace_id 
		BoundedVec<[u8;32],T::MaxApplicationsPerCustodian>, //application_id 
		ValueQuery
	>;




	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Marketplaces stored. [owner, admin, market_id]
		MarketplaceStored(T::AccountId, T::AccountId, [u8;32]),
		/// Application stored on the specified marketplace. [application_id, market_id]
		ApplicationStored([u8;32], [u8;32]),
		/// An applicant was accepted or rejected on the marketplace. [AccountOrApplication, market_id, status]
		ApplicationProcessed(AccountOrApplication<T>,[u8;32], ApplicationStatus),
		/// Add a new authority to the selected marketplace
		AuthorityAdded(T::AccountId, MarketplaceAuthority),
		/// Remove the selected authority from the selected marketplace
		AuthorityRemoved(T::AccountId, MarketplaceAuthority),
	}

	// Errors inform users that something went wrong.
	#[pallet::error]
	pub enum Error<T> {
		/// Work In Progress
		NotYetImplemented,
		/// Error names should be descriptive.
		NoneValue,
		/// The account supervises too many marketplaces
		ExceedMaxMarketsPerAuth,
		/// The account has too many roles in that marketplace 
		ExceedMaxRolesPerAuth,
		/// Too many applicants for this market! try again later
		ExceedMaxApplicants,
		/// This custodian has too many applications for this market, try with another one
		ExceedMMaxApplicationsPerCustodian,
		/// Applicaion doesnt exist
		ApplicationNotFound,
		/// The user has not applicated to that market before
		ApplicantNotFound,
		/// A marketplace with the same data exists already
		MarketplaceAlreadyExists,
		/// The user has already applied to the marketplace (or an identical application exist)
		AlreadyApplied,
		/// The specified marketplace does not exist
		MarketplaceNotFound,
		/// You need to be an owner or an admin of the marketplace
		CannotEnroll,
		/// There cannot be more than one owner per marketplace
		OnlyOneOwnerIsAllowed,
		/// Cannot remove the owner of the marketplace
		CantRemoveOwner,
		/// Admin can not remove itself
		NegateRemoveAdminItself,
		/// User has already been assigned with that role
		CannotAddAuthority,
		/// User not found
		UserNotFound,
		// Rol not found for the selected user
		RolNotFoundForUser,
		/// User is not admin	
		UserIsNotAdmin,
		/// User is not found for the query
		UserNotFoundForThisQuery
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {

		#[transactional]
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn create_marketplace(origin: OriginFor<T>, admin: T::AccountId,label: BoundedVec<u8,T::LabelMaxLen>) -> DispatchResult {
			let who = ensure_signed(origin)?; // origin will be market owner
			let m = Marketplace{
				label,
			};
			Self::do_create_marketplace(who, admin, m)
		}
		
		#[transactional]
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn apply(
			origin: OriginFor<T>, 
			marketplace_id: [u8;32],
			// Getting encoding errors from polkadotjs if an object vector have optional fields
			fields : BoundedVec<(BoundedVec<u8,ConstU32<100> >,BoundedVec<u8,ConstU32<100>> ), T::MaxFiles>,
			custodian_fields: Option<(T::AccountId, BoundedVec<BoundedVec<u8,ConstU32<100>>, T::MaxFiles> )> 
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let (custodian, fields) = Self::set_up_application(fields,custodian_fields);
			let application = Application::<T>{
				status: ApplicationStatus::default(),
				fields ,
			};
			Self::do_apply(who, custodian, marketplace_id, application)
		}

		#[transactional]
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn enroll(origin: OriginFor<T>, marketplace_id: [u8;32], account_or_application: AccountOrApplication<T>, approved: bool ) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Self::do_enroll(who, marketplace_id, account_or_application, approved)
		}

		#[transactional]
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn add_authority(origin: OriginFor<T>, author: T::AccountId, authority_type: MarketplaceAuthority, marketplace_id: [u8;32]) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Self::do_authorise(who, author, authority_type, marketplace_id)
		}


		#[transactional]
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn remove_authority(origin: OriginFor<T>, author: T::AccountId, authority_type: MarketplaceAuthority, marketplace_id: [u8;32]) -> DispatchResult {
			let who = ensure_signed(origin)?;
			//TOREVIEW: If we're allowing more than one role per user per marketplace, we should 
			// check what role we want to remove instead of removing the user completely from
			// selected marketplace. 
			Self::remove_authorise(who, author, authority_type, marketplace_id)
		}

		#[transactional]
		#[pallet::weight(10_000 + T::DbWeight::get().writes(1))]
		pub fn kill_storage(
			origin: OriginFor<T>,
		) -> DispatchResult{
			T::RemoveOrigin::ensure_origin(origin.clone())?;
			<Marketplaces<T>>::remove_all(None);
			<MarketplacesByAuthority<T>>::remove_all(None);
			<AuthoritiesByMarketplace<T>>::remove_all(None);
			<Applications<T>>::remove_all(None);
			<ApplicationsByAccount<T>>::remove_all(None);
			<ApplicantsByMarketplace<T>>::remove_all(None);
			<Custodians<T>>::remove_all(None);
			Ok(())
		}


	}
}