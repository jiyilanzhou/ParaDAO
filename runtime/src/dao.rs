use parity_codec::{Decode, Encode};
use support::{StorageValue, StorageMap, dispatch::Result, decl_module, decl_storage, decl_event, ensure};
use support::traits::{Currency, WithdrawReason, ExistenceRequirement};
use runtime_primitives::traits::{As, CheckedSub, CheckedAdd, CheckedMul, CheckedDiv};
use system::ensure_signed;
use rstd::vec::Vec;

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct Member {
    energy: u64,
    highest_index_yes_vote: u32,
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct AccessProposal<AccountId, Balance> {
    proposer: AccountId,
    applicant: AccountId,
    energies_requested: u64,
    mortgage: Balance,
    deposit: Balance,
    starting_period: u64,
    yes_votes: u64,
    no_votes: u64,
    processed: bool,
    did_pass: bool,
    aborted: bool,
    detail: Vec<u8>,
}

#[derive(Encode, Decode, Default, Clone, PartialEq)]
#[cfg_attr(feature = "std", derive(Debug))]
pub struct ProjectProposal<AccountId, Balance> {
    proposer: AccountId,
    applicant: AccountId,
    mortgage: Balance,
    starting_period: u64,
    milestone_1_requested: Balance,
    milestone_2_requested: Balance,
    milestone_3_requested: Balance,
    yes_votes: u64,
    no_votes: u64,
    processed: bool,
    stage_did_pass: bool,
    round: u64,
    aborted: bool,
    status: ProjectStatus,
    detail: Vec<u8>,
}

#[derive(Encode, Decode, Clone, PartialEq, Eq, Copy)]
#[cfg_attr(feature = "std", derive(Debug))]
pub enum ProjectStatus {
    Initialization,
    Milestone1,
    Milestone2,
    Milestone3,
}

impl Default for ProjectStatus {
    fn default() -> Self { ProjectStatus::Initialization }
}

pub trait Trait: balances::Trait + timestamp::Trait {
    type Event: From<Event<Self>> + Into<<Self as system::Trait>::Event>;
}

decl_event! (
    pub enum Event<T>
    where AccountId = <T as system::Trait>::AccountId,
    Balance = <T as balances::Trait>::Balance,
    {
        SummonComplete(AccountId),
        Approval(AccountId, Balance),
        Donate(AccountId, Balance),
        SubmitAccessProposal(AccountId, AccountId, u64, Balance, u64),
        SubmitProjectProposal(AccountId, AccountId, Balance, Balance, Balance, u64),
        ForwardToMilestone(AccountId, u32, ProjectStatus, u64, u64),
        AccessVote(AccountId, u32, bool),
        ProjectVote(AccountId, u32, ProjectStatus, u64, bool),
        AccessAbort(u32),
        RageQuit(AccountId, u64, Balance),
        NewMember(AccountId, u64),
        ProcessAccessProposal(u32, AccountId, AccountId, Balance, u64, bool),
        ProcessProjectProposal(u32, AccountId, AccountId, ProjectStatus, u64, bool, Balance),
    }
);

decl_storage! {
    trait Store for Module<T: Trait> as Dao {
        // assert pool
        FreePool get(free_pool): T::Balance;
        MortgagePool get(mortgage_pool): T::Balance;
        DepositPool get(deposit_pool): T::Balance;
        GrantLockedPool get(grant_locked_pool): T::Balance;

        // energy pool - voting weight
        TotalEnergies get(total_energies): u64;
        TotalEnergiesRequested get(total_energies_requested): u64;

        // allowance
        Allowance get(allowance): map T::AccountId => T::Balance;

        // members
        MembersCount get(members_count): u32;
        MembersArray get(members_array): map u32 => T::AccountId;
        Members get(members): map T::AccountId => Member;

        // access proposal
        AccessProposalsCount get(access_proposals_count): u32;
        AccessProposals get(access_proposals): map u32 => AccessProposal<T::AccountId, T::Balance>;
        ProcessedAccessProposalsCount get(processed_access_proposals_count): u32;

        // project proposal
        ProjectProposalsCount get(project_proposals_count): u32;
        ProjectProposals get(project_proposals): map u32 => ProjectProposal<T::AccountId, T::Balance>;

        ProjectsProcessQueue get(projects_process_queue): map u32 => u32;
        UnprocessedQueueHead get(unprocssed_queue_head): u32;
        UnprocessedQueueLength get(unprocssed_queue_length): u32;
        

        // vote
        VotesForAccess get(votes_for_access): map (u32, T::AccountId) => Option<bool>;
        VotesForProject get(votes_for_project): map (u32, T::AccountId, ProjectStatus, u64) => Option<bool>;

        // detail
        Summoner get(summoner): Option<T::AccountId>;
        SummoningTime get(summonging_time): T::Moment;

        // config
        PeriodDuration get(period_duration) config(): T::Moment;
        VotingPeriodLength get(voting_period_length) config(): u64;
        AbortWindow get(abort_window) config(): u64;
        ProposalMortgage get(proposal_mortgage) config(): T::Balance;
    }
}

decl_module! {
    pub struct Module<T: Trait> for enum Call where origin: T::Origin {
        fn deposit_event<T>() = default;

        pub fn summon(origin) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(Self::summoner() == None, "The dao has been summoned!");

            let now = <timestamp::Module<T>>::get();
            let summoner = Member{
                energy: 1,
                highest_index_yes_vote: 0,
            };

            <Summoner<T>>::put(sender.clone());
            <SummoningTime<T>>::put(now.clone());
            <TotalEnergies<T>>::put(summoner.energy);
            <Members<T>>::insert(sender.clone(), summoner);
            <MembersArray<T>>::insert(Self::members_count(), sender.clone());
            <MembersCount<T>>::mutate(|n| *n += 1);

            Self::deposit_event(RawEvent::SummonComplete(sender));
            Ok(())
        }

        pub fn applicant_approve(origin, value: T::Balance) -> Result {
            let sender = ensure_signed(origin)?;

            <Allowance<T>>::insert(sender.clone(), value);

            Self::deposit_event(RawEvent::Approval(sender, value));
            Ok(())
        }

        pub fn donate(origin, value: T::Balance) -> Result {
            let sender = ensure_signed(origin)?;

            let new_free_pool = Self::free_pool().checked_add(&value).ok_or("overflow in calculating free pool")?;
            let _ = <balances::Module<T> as Currency<_>>::withdraw(&sender, value, WithdrawReason::Reserve, ExistenceRequirement::KeepAlive)?;
            <FreePool<T>>::put(new_free_pool);
    
            Self::deposit_event(RawEvent::Donate(sender, value));
            Ok(())
        }

        pub fn submit_access_proposal(
            origin, 
            applicant: T::AccountId,
            deposit: T::Balance,
            energies_requested: u64,
            detail: Vec<u8>
        ) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(Self::is_member(&sender), "Sender is not a member");
            ensure!(applicant != T::AccountId::default(), "Applicant is not set");
            ensure!(Self::allowance(applicant.clone()) >= deposit, "Allowance of the applicant is not enough");

            let total_energies_requested = Self::total_energies_requested();
            let new_total_energies_requested = total_energies_requested.checked_add(energies_requested).ok_or("overflow in calculating energies")?;

            let mut new_deposit_pool = Self::deposit_pool();
            if deposit > <T::Balance as As<u64>>::sa(0) {
                new_deposit_pool = new_deposit_pool.checked_add(&deposit).ok_or("overflow in calculating deposit pool")?;
            }

            let mut this_starting_period: u64 = Self::get_current_period();
            if Self::access_proposals_count() != 0 && Self::access_proposals(Self::access_proposals_count() - 1).starting_period > this_starting_period {
                this_starting_period = Self::access_proposals(Self::access_proposals_count()-1).starting_period + 1;
            }

            let new_mortgage_pool = Self::mortgage_pool().checked_add(&Self::proposal_mortgage()).ok_or("overflow in calculating mortgage pool")?;

            // create proposal
            let access_proposal = AccessProposal {
                proposer: sender.clone(),
                applicant: applicant.clone(),
                energies_requested: energies_requested,
                mortgage: Self::proposal_mortgage(),
                deposit: deposit,
                starting_period: this_starting_period,
                yes_votes: 0,
                no_votes: 0,
                processed: false,
                did_pass: false,
                aborted: false,
                detail: detail,
            };

            if deposit > <T::Balance as As<u64>>::sa(0) {
                let _ = <balances::Module<T> as Currency<_>>::withdraw(&applicant, deposit, WithdrawReason::Reserve, ExistenceRequirement::KeepAlive)?;
                <DepositPool<T>>::put(new_deposit_pool);
                <Allowance<T>>::mutate(applicant.clone(), |n| *n -= deposit);
            }
            let _ = <balances::Module<T> as Currency<_>>::withdraw(&sender, access_proposal.mortgage, WithdrawReason::Reserve, ExistenceRequirement::KeepAlive)?;
            <MortgagePool<T>>::put(new_mortgage_pool);
            <TotalEnergiesRequested<T>>::put(new_total_energies_requested);
            <AccessProposals<T>>::insert(Self::access_proposals_count(), access_proposal);
            <AccessProposalsCount<T>>::mutate(|n| *n += 1);

            Self::deposit_event(RawEvent::SubmitAccessProposal(sender, applicant, energies_requested, deposit, this_starting_period));
            Ok(())
        }

        pub fn submit_project_proposal(
            origin, 
            applicant: T::AccountId,
            milestone_1_requested: T::Balance,
            milestone_2_requested: T::Balance,
            milestone_3_requested: T::Balance,
            detail: Vec<u8>
        ) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(Self::is_member(&sender), "Sender is not a member");
            ensure!(applicant != T::AccountId::default(), "Applicant is not set");

            let mut this_starting_period: u64 = Self::get_current_period();
            if Self::unprocssed_queue_length() != 0 && Self::project_proposals(Self::projects_process_queue(Self::unprocssed_queue_head() + Self::unprocssed_queue_length() - 1)).starting_period > this_starting_period {
                this_starting_period = Self::project_proposals(Self::projects_process_queue(Self::unprocssed_queue_head() + Self::unprocssed_queue_length() - 1)).starting_period + 1;
            }

            let new_mortgage_pool = Self::mortgage_pool().checked_add(&Self::proposal_mortgage()).ok_or("overflow in calculating mortgage pool")?;

            // create proposal
            let project_proposal = ProjectProposal {
                proposer: sender.clone(),
                applicant: applicant.clone(),
                mortgage: Self::proposal_mortgage(),
                starting_period: this_starting_period,
                milestone_1_requested: milestone_1_requested,
                milestone_2_requested: milestone_2_requested,
                milestone_3_requested: milestone_3_requested,
                yes_votes: 0,
                no_votes: 0,
                processed: false,
                stage_did_pass: false,
                round: 0,
                aborted: false,
                status: ProjectStatus::Initialization,
                detail: detail,
            };

            let _ = <balances::Module<T> as Currency<_>>::withdraw(&sender, project_proposal.mortgage, WithdrawReason::Reserve, ExistenceRequirement::KeepAlive)?;
            <MortgagePool<T>>::put(new_mortgage_pool);
            <ProjectProposals<T>>::insert(Self::project_proposals_count(), project_proposal);
            <ProjectsProcessQueue<T>>::insert(Self::unprocssed_queue_head() + Self::unprocssed_queue_length(), Self::project_proposals_count());
            <UnprocessedQueueLength<T>>::mutate(|n| *n += 1);
            <ProjectProposalsCount<T>>::mutate(|n| *n += 1);

            Self::deposit_event(RawEvent::SubmitProjectProposal(sender, applicant, milestone_1_requested, milestone_2_requested, milestone_3_requested, this_starting_period));
            Ok(())
        }

        pub fn forward_to_milestone(origin, project_proposal_index: u32) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(Self::is_member(&sender), "Sender is not a member");
            ensure!(<ProjectProposals<T>>::exists(project_proposal_index), "This project proposal not exists!");
            let mut project_proposal = Self::project_proposals(project_proposal_index);

            ensure!(project_proposal.processed, "Must forward project until be processed!");
            ensure!(!project_proposal.aborted, "This project has been aborted");

            if project_proposal.stage_did_pass {
                match project_proposal.status {
                    ProjectStatus::Initialization => { 
                            project_proposal.status = ProjectStatus::Milestone1;
                        },
                    ProjectStatus::Milestone1 => { 
                            project_proposal.status = ProjectStatus::Milestone2;
                        },
                    ProjectStatus::Milestone2 => { 
                            project_proposal.status = ProjectStatus::Milestone3;
                        },
                    ProjectStatus::Milestone3 => { return Err("This project is completely done!") },
                }
                project_proposal.round = 0;
            }else {
                project_proposal.round += 1;
            }

            let grant_locked = match project_proposal.status {
                ProjectStatus::Milestone1 => project_proposal.milestone_1_requested,
                ProjectStatus::Milestone2 => project_proposal.milestone_2_requested,
                ProjectStatus::Milestone3 => project_proposal.milestone_3_requested,
                _ => <T::Balance as As<u64>>::sa(0),
            };
            project_proposal.stage_did_pass = false;
            project_proposal.processed = false;
            project_proposal.yes_votes = 0;
            project_proposal.no_votes = 0;
            
            let mut this_starting_period: u64 = Self::get_current_period();
            if Self::unprocssed_queue_length() != 0 && Self::project_proposals(Self::projects_process_queue(Self::unprocssed_queue_head() + Self::unprocssed_queue_length() - 1)).starting_period > this_starting_period {
                this_starting_period = Self::project_proposals(Self::projects_process_queue(Self::unprocssed_queue_head() + Self::unprocssed_queue_length() - 1)).starting_period + 1;
            }
            project_proposal.starting_period = this_starting_period;

            if grant_locked > <T::Balance as As<u64>>::sa(0) {
                ensure!(Self::free_pool() >= grant_locked, "Free pool is insufficient!");
                <FreePool<T>>::mutate(|n| *n -= grant_locked);
                <GrantLockedPool<T>>::mutate(|n| *n += grant_locked);
            }
            <ProjectProposals<T>>::insert(project_proposal_index, project_proposal.clone());
            <ProjectsProcessQueue<T>>::insert(Self::unprocssed_queue_head() + Self::unprocssed_queue_length(), project_proposal_index);
            <UnprocessedQueueLength<T>>::mutate(|n| *n += 1);

            Self::deposit_event(RawEvent::ForwardToMilestone(sender, project_proposal_index, project_proposal.status, project_proposal.round, this_starting_period));
            Ok(())
        }

        pub fn submit_access_vote(
            origin, 
            access_proposal_index: u32, 
            vote: bool
        ) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<AccessProposals<T>>::exists(access_proposal_index), "access proposal index is invalid!");
            let mut access_proposal = Self::access_proposals(access_proposal_index);
            ensure!(Self::in_vote_period(access_proposal.starting_period), "Not in voting period!");
            ensure!(!access_proposal.aborted, "The access proposal has been aborted!");
            ensure!(Self::is_member(&sender), "Sender is not a member");
            ensure!(!<VotesForAccess<T>>::exists((access_proposal_index, sender.clone())), "already voted!");

            let mut member = Self::members(sender.clone());

            if vote {
                access_proposal.yes_votes += member.energy;
            }else {
                access_proposal.no_votes += member.energy;
            }

            if vote && access_proposal_index > member.highest_index_yes_vote {
                member.highest_index_yes_vote = access_proposal_index;
                <Members<T>>::insert(sender.clone(), member);
            }
            <AccessProposals<T>>::insert(access_proposal_index, access_proposal);
            <VotesForAccess<T>>::insert((access_proposal_index, sender.clone()), vote);

            Self::deposit_event(RawEvent::AccessVote(sender, access_proposal_index, vote));
            Ok(())
        }

        pub fn submit_project_vote(
            origin,
            project_proposal_index: u32,
            vote: bool
        ) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<ProjectProposals<T>>::exists(project_proposal_index), "project proposal index is invalid!");
            let mut project_proposal = Self::project_proposals(project_proposal_index);
            ensure!(Self::in_vote_period(project_proposal.starting_period), "Not in voting period!");
            ensure!(!project_proposal.aborted, "The project proposal has been aborted!");
            ensure!(Self::is_member(&sender), "Sender is not a member");
            ensure!(!<VotesForProject<T>>::exists((project_proposal_index, sender.clone(), project_proposal.status, project_proposal.round)), "already voted for this round!");

            let member = Self::members(sender.clone());
            if vote {
                project_proposal.yes_votes += member.energy;
            }else {
                project_proposal.no_votes += member.energy;
            }
            
            <ProjectProposals<T>>::insert(project_proposal_index, project_proposal.clone());
            <VotesForProject<T>>::insert((project_proposal_index, sender.clone(), project_proposal.status.clone(), project_proposal.round), vote);

            Self::deposit_event(RawEvent::ProjectVote(sender, project_proposal_index, project_proposal.status.clone(), project_proposal.round, vote));
            Ok(())
        }

        pub fn abort_access(origin, access_proposal_index: u32) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(<AccessProposals<T>>::exists(access_proposal_index), "access proposal index is invalid!");
            let mut access_proposal = Self::access_proposals(access_proposal_index);
            ensure!(sender == access_proposal.applicant, "You are not the applicant of this access proposal!");
            ensure!(!access_proposal.aborted, "This access proposal has been aborted!");
            ensure!(Self::get_current_period() < (access_proposal.starting_period + Self::abort_window()), "Abort window has passed!");

            access_proposal.aborted = true;
            let deposit_return = access_proposal.deposit;
            access_proposal.deposit = <T::Balance as As<u64>>::sa(0);

            if deposit_return > <T::Balance as As<u64>>::sa(0) {
                let new_deposit_pool = Self::deposit_pool().checked_sub(&deposit_return).ok_or("overflow in calculating deposit return")?;
                let _ = <balances::Module<T> as Currency<_>>::deposit_into_existing(&sender, deposit_return)
                    .expect("`sender` must exist since a transaction is being made and withdraw will keep alive; qed.");
                <DepositPool<T>>::put(new_deposit_pool);
            }
            <AccessProposals<T>>::insert(access_proposal_index, access_proposal);

            Self::deposit_event(RawEvent::AccessAbort(access_proposal_index));
            Ok(())
        }

        pub fn rage_quit(origin, energies_to_burn: u64) -> Result {
            let sender = ensure_signed(origin)?;

            ensure!(Self::is_member(&sender), "Sender must be member!");
            ensure!(energies_to_burn > 0, "energies to burn must more than 0");
            let mut member = Self::members(sender.clone());
            ensure!(member.energy >= energies_to_burn, "Energy is not enough");
            ensure!(Self::access_proposals(member.highest_index_yes_vote).processed, "cant ragequit until highest index proposal member voted YES on is processed");

            member.energy = member.energy.checked_sub(energies_to_burn).ok_or("overflow in calculating energy")?;
            let new_total_energies = Self::total_energies() - energies_to_burn;
            let redeem_balance = Self::free_pool().checked_mul(&<T::Balance as As<u64>>::sa(energies_to_burn)).ok_or("overflow in calculating redeem")?
                                    .checked_div(&<T::Balance as As<u64>>::sa(Self::total_energies())).ok_or("overflow in calculating redeem")?;
            let new_free_pool = Self::free_pool().checked_sub(&redeem_balance).ok_or("overflow in calculating free pool")?;

            if redeem_balance > <T::Balance as As<u64>>::sa(0) {
                let _ = <balances::Module<T> as Currency<_>>::deposit_into_existing(&sender, redeem_balance)
                    .expect("`sender` must exist since a transaction is being made and withdraw will keep alive; qed.");
                <FreePool<T>>::put(new_free_pool);
            }
            <Members<T>>::insert(sender.clone(), member);
            <TotalEnergies<T>>::put(new_total_energies);
            
            Self::deposit_event(RawEvent::RageQuit(sender, energies_to_burn, redeem_balance));
            Ok(())
        }

        fn on_initialize() {
            let current_period = Self::get_current_period();


            // process access proposal
            let processed_access_proposals_count = Self::processed_access_proposals_count();
            if Self::access_proposals_count() > processed_access_proposals_count{
                // judge to process access proposal
                let mut first_unprocessed_access_proposal = Self::access_proposals(processed_access_proposals_count);
                if current_period >= first_unprocessed_access_proposal.starting_period + Self::voting_period_length() {
                    // process the first_unprocessed_access_proposal
                    first_unprocessed_access_proposal.processed = true;
                    first_unprocessed_access_proposal.did_pass = (first_unprocessed_access_proposal.yes_votes > first_unprocessed_access_proposal.no_votes)
                                                        && !first_unprocessed_access_proposal.aborted;

                    <TotalEnergiesRequested<T>>::mutate(|n| *n -= first_unprocessed_access_proposal.energies_requested);

                    if first_unprocessed_access_proposal.did_pass {
                        
                        if <Members<T>>::exists(first_unprocessed_access_proposal.applicant.clone()) {
                            // member already exists
                            let mut member = Self::members(first_unprocessed_access_proposal.applicant.clone());
                            member.energy += first_unprocessed_access_proposal.energies_requested;

                            <Members<T>>::insert(first_unprocessed_access_proposal.applicant.clone(), member.clone());
                            // mint new energies
                            <TotalEnergies<T>>::mutate(|n| *n += first_unprocessed_access_proposal.energies_requested);
                        } else {
                            // the applicant is a new member, create a new record
                            let member = Member {
                                energy: first_unprocessed_access_proposal.energies_requested,
                                highest_index_yes_vote: 0,
                            };

                            <TotalEnergies<T>>::mutate(|n| *n += first_unprocessed_access_proposal.energies_requested);
                            
                            <Members<T>>::insert(first_unprocessed_access_proposal.applicant.clone(), member);
                            <MembersArray<T>>::insert(Self::members_count(), first_unprocessed_access_proposal.applicant.clone());
                            <MembersCount<T>>::mutate(|n| *n += 1);

                            Self::deposit_event(RawEvent::NewMember(first_unprocessed_access_proposal.applicant.clone(), first_unprocessed_access_proposal.energies_requested));
                        }

                        if first_unprocessed_access_proposal.deposit > <T::Balance as As<u64>>::sa(0) {
                            // move deposit from deposit pool to free balance
                            <DepositPool<T>>::mutate(|n| *n -= first_unprocessed_access_proposal.deposit);
                            <FreePool<T>>::mutate(|n| *n += first_unprocessed_access_proposal.deposit);
                        }

                    } else {
                        // return deposit to applicant
                        if first_unprocessed_access_proposal.deposit > <T::Balance as As<u64>>::sa(0) {
                            <DepositPool<T>>::mutate(|n| *n -= first_unprocessed_access_proposal.deposit);
                            let _ = <balances::Module<T> as Currency<_>>::deposit_creating(&first_unprocessed_access_proposal.applicant.clone(), first_unprocessed_access_proposal.deposit);
                        }
                    }

                    // update access proposal
                    let _ = <balances::Module<T> as Currency<_>>::deposit_creating(&first_unprocessed_access_proposal.proposer, first_unprocessed_access_proposal.mortgage);
                    <MortgagePool<T>>::mutate(|n| *n -= first_unprocessed_access_proposal.mortgage);
                    <AccessProposals<T>>::insert(processed_access_proposals_count, first_unprocessed_access_proposal.clone());
                    <ProcessedAccessProposalsCount<T>>::mutate(|n| *n += 1);
                
                    Self::deposit_event(RawEvent::ProcessAccessProposal(processed_access_proposals_count, first_unprocessed_access_proposal.proposer, first_unprocessed_access_proposal.applicant.clone(), 
                    first_unprocessed_access_proposal.deposit, first_unprocessed_access_proposal.energies_requested, first_unprocessed_access_proposal.did_pass));
                }
            }

            // ProjectsProcessQueue get(projects_process_queue): map u32 => u32;
            // UnprocessedQueueHead get(unprocssed_queue_head): u32;
            // UnprocessedQueueLength get(unprocssed_queue_length): u32;

            // process project proposal
            let unprocessed_queue_length = Self::unprocssed_queue_length();
            if unprocessed_queue_length > 0 {
                let first_unprocessed_project_proposal_index = Self::projects_process_queue(Self::unprocssed_queue_head());
                let mut first_unprocessed_project_proposal = Self::project_proposals(first_unprocessed_project_proposal_index);

                if current_period >= first_unprocessed_project_proposal.starting_period + Self::voting_period_length() {
                    // process the project
                    first_unprocessed_project_proposal.processed = true;
                    first_unprocessed_project_proposal.stage_did_pass = (first_unprocessed_project_proposal.yes_votes > first_unprocessed_project_proposal.no_votes)
                                                        && !first_unprocessed_project_proposal.aborted;

                    let grant_this_stage = match first_unprocessed_project_proposal.status {
                        ProjectStatus::Initialization => <T::Balance as As<u64>>::sa(0),
                        ProjectStatus::Milestone1 => first_unprocessed_project_proposal.milestone_1_requested,
                        ProjectStatus::Milestone2 => first_unprocessed_project_proposal.milestone_2_requested,
                        ProjectStatus::Milestone3 => first_unprocessed_project_proposal.milestone_3_requested,
                    };
                    if first_unprocessed_project_proposal.stage_did_pass {
                        if first_unprocessed_project_proposal.status == ProjectStatus::Milestone3 {
                            first_unprocessed_project_proposal.aborted = true;
                            let _ = <balances::Module<T> as Currency<_>>::deposit_creating(&first_unprocessed_project_proposal.proposer, first_unprocessed_project_proposal.mortgage);
                            <MortgagePool<T>>::mutate(|n| *n -= first_unprocessed_project_proposal.mortgage);
                        }

                        if grant_this_stage > <T::Balance as As<u64>>::sa(0) {
                            <GrantLockedPool<T>>::mutate(|n| *n -= grant_this_stage);
                            let _ = <balances::Module<T> as Currency<_>>::deposit_creating(&first_unprocessed_project_proposal.applicant.clone(), grant_this_stage);
                        }
                    }else {
                        if grant_this_stage > <T::Balance as As<u64>>::sa(0) {
                            <GrantLockedPool<T>>::mutate(|n| *n -= grant_this_stage);
                            <FreePool<T>>::mutate(|n| *n += grant_this_stage);
                        }
                    }

                    // update project proposal
                    <ProjectProposals<T>>::insert(first_unprocessed_project_proposal_index, first_unprocessed_project_proposal.clone());
                    <UnprocessedQueueHead<T>>::mutate(|n| *n += 1);
                    <UnprocessedQueueLength<T>>::mutate(|n| *n -= 1);

                    Self::deposit_event(RawEvent::ProcessProjectProposal(
                        first_unprocessed_project_proposal_index,
                        first_unprocessed_project_proposal.proposer, 
                        first_unprocessed_project_proposal.applicant, 
                        first_unprocessed_project_proposal.status, 
                        first_unprocessed_project_proposal.round,
                        first_unprocessed_project_proposal.stage_did_pass,
                        grant_this_stage));
                }
            }
        }
    }
}

impl<T: Trait> Module<T> {
    pub fn is_member(account: &T::AccountId) -> bool {
        <Members<T>>::exists(account) && Self::members(account).energy > 0
    }

    pub fn get_current_period() -> u64 {
        <T::Moment as As<u64>>::as_(<timestamp::Module<T>>::get() / Self::period_duration())
    }

    pub fn has_voting_period_expired(starting_period: u64) -> bool {
        Self::get_current_period() >= (starting_period + Self::voting_period_length())
    }

    pub fn in_vote_period(starting_period: u64) -> bool {
        Self::get_current_period() >= starting_period && !Self::has_voting_period_expired(starting_period)
    }
}