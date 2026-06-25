# Fix CI Failures Plan

## File: contracts/escrow/src/lib.rs

### Fix 1: Redundant closure (line 130)
Replace:
```
        .unwrap_or_else(|| default_fee_config())
```
With:
```
        .unwrap_or_else(default_fee_config)
```

### Fix 2: Add `#[allow(clippy::too_many_arguments)]` above `impl Escrow`
Around line 102, change:
```
#[contract]
pub struct Escrow;
```
To:
```
#[contract]
pub struct Escrow;

#[allow(clippy::too_many_arguments)]
```

### Fix 3: len_zero (line 669)
Replace:
```
        if tracking_id.len() == 0 {
```
With:
```
        if tracking_id.is_empty() {
```

### Fix 4: Restore `fund_escrow` (after create_escrow, around line 629)

Insert after the closing `}` of `create_escrow` (after `Ok(escrow_id)` at line 628):

```
    #[allow(clippy::too_many_arguments)]
    pub fn fund_escrow(
        env: Env,
        escrow_id: u64,
        buyer: Address,
    ) -> Result<(), ContractError> {
        buyer.require_auth();
        ensure_not_paused(&env)?;

        let mut escrow = load_escrow(&env, escrow_id)?;

        if escrow.state != EscrowState::Pending {
            return Err(ContractError::InvalidState);
        }

        escrow.buyer = Some(buyer.clone());
        escrow.state = EscrowState::Funded;
        escrow.funded_at = env.ledger().timestamp();
        escrow.dispute_deadline = escrow.funded_at + DISPUTE_WINDOW;

        let token_client = token::Client::new(&env, &escrow.token);
        token_client.transfer(&buyer, &env.current_contract_address(), &escrow.amount);

        save_escrow(&env, escrow_id, &escrow);

        let mut buyer_escrows: Vec<u64> = env.storage().persistent()
            .get(&DataKey::BuyerEscrowIndex(buyer.clone()))
            .unwrap_or(Vec::new(&env));
        buyer_escrows.push_back(escrow_id);
        env.storage().persistent().set(&DataKey::BuyerEscrowIndex(buyer.clone()), &buyer_escrows);

        emit_escrow_funded(&env, escrow_id, buyer, escrow.amount);
        Ok(())
    }
```

### Fix 5: Restore `raise_dispute` (after fund_escrow)

```
    pub fn raise_dispute(
        env: Env,
        caller: Address,
        escrow_id: u64,
        reason: Symbol,
        description: String,
        evidence_hash: BytesN<32>,
    ) -> Result<(), ContractError> {
        caller.require_auth();
        ensure_not_paused(&env)?;

        let mut escrow = load_escrow(&env, escrow_id)?;

        let buyer = escrow.buyer.clone().ok_or(ContractError::EscrowHasNoBuyer)?;
        if caller != buyer {
            return Err(ContractError::NotAuthorized);
        }

        if escrow.state != EscrowState::Shipped {
            return Err(ContractError::InvalidState);
        }

        if env.ledger().timestamp() >= escrow.dispute_deadline {
            return Err(ContractError::DisputeWindowClosed);
        }

        if description.len() > MAX_DESCRIPTION_LEN {
            return Err(ContractError::InputTooLong);
        }

        escrow.state = EscrowState::Disputed;
        let now = env.ledger().timestamp();

        let dispute = DisputeData {
            escrow_id,
            reason,
            description,
            evidence_hash,
            status: DisputeStatus::Active,
            disputed_at: now,
            tracking_id: escrow.tracking_id.clone(),
        };

        save_escrow(&env, escrow_id, &escrow);
        save_dispute(&env, escrow_id, &dispute);
        increment_counter(&env, &DataKey::TotalDisputed)?;
        emit_dispute_raised(&env, escrow_id, caller, reason);
        Ok(())
    }
```

## Verification
After applying all 5 fixes, run:
```
cargo clippy --workspace -- -D warnings
cargo test --workspace
```
