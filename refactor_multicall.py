import re
import os

path = r'c:\Users\supre\Documents\trust-link-contract\contracts\escrow\src\instructions.rs'
with open(path, 'r', encoding='utf-8') as f:
    content = f.read()

# I want to extract the multicall function block and replace it.
# The block starts at `pub fn multicall(env: Env, calls: Vec<ContractCall>) -> Result<Vec<Val>, ContractError> {`
# and ends when we find the matching `}` for `pub fn multicall`

start_idx = content.find('pub fn multicall(env: Env, calls: Vec<ContractCall>) -> Result<Vec<Val>, ContractError> {')
if start_idx == -1:
    print("Could not find multicall")
    exit(1)

# Find end
brace_count = 0
end_idx = -1
for i in range(start_idx, len(content)):
    if content[i] == '{':
        brace_count += 1
    elif content[i] == '}':
        brace_count -= 1
        if brace_count == 0:
            end_idx = i + 1
            break

multicall_body = content[start_idx:end_idx]

helper = """
fn parse_arg<T: TryFromVal<Env, Val>>(
    env: &Env,
    args: &Vec<Val>,
    idx: u32,
) -> Result<T, ContractError> {
    args.get(idx)
        .ok_or(ContractError::InvalidMulticallArg)?
        .try_into_val(env)
        .map_err(|_| ContractError::InvalidMulticallArg)
}
"""

new_multicall = """    pub fn multicall(env: Env, calls: Vec<ContractCall>) -> Result<Vec<Val>, ContractError> {
        ensure_not_paused(&env)?;
        let mut results = Vec::new(&env);

        let s_initialize = Symbol::new(&env, "initialize");
        let s_pause_contract = Symbol::new(&env, "pause_contract");
        let s_unpause_contract = Symbol::new(&env, "unpause_contract");
        let s_create_escrow = Symbol::new(&env, "create_escrow");
        let s_fund_escrow = Symbol::new(&env, "fund_escrow");
        let s_mark_shipped = Symbol::new(&env, "mark_shipped");
        let s_confirm_delivery = Symbol::new(&env, "confirm_delivery");
        let s_raise_dispute = Symbol::new(&env, "raise_dispute");
        let s_resolve_dispute = Symbol::new(&env, "resolve_dispute");
        let s_auto_release = Symbol::new(&env, "auto_release");
        let s_get_escrow = Symbol::new(&env, "get_escrow");
        let s_get_dispute = Symbol::new(&env, "get_dispute");
        let s_get_fee_config = Symbol::new(&env, "get_fee_config");
        let s_set_arbitration_fee = Symbol::new(&env, "set_arbitration_fee");
        let s_get_arbitration_fee = Symbol::new(&env, "get_arbitration_fee");
        let s_rotate_resolver = Symbol::new(&env, "rotate_resolver");
        let s_cancel_escrow = Symbol::new(&env, "cancel_escrow");

        for call in calls.into_iter() {
            let res_val: Val = if call.function == s_fund_escrow {
                dispatch_fund_escrow(&env, &call.args)?
            } else if call.function == s_get_escrow {
                dispatch_get_escrow(&env, &call.args)?
            } else if call.function == s_mark_shipped {
                dispatch_mark_shipped(&env, &call.args)?
            } else if call.function == s_confirm_delivery {
                dispatch_confirm_delivery(&env, &call.args)?
            } else if call.function == s_raise_dispute {
                dispatch_raise_dispute(&env, &call.args)?
            } else if call.function == s_resolve_dispute {
                dispatch_resolve_dispute(&env, &call.args)?
            } else if call.function == s_auto_release {
                dispatch_auto_release(&env, &call.args)?
            } else if call.function == s_cancel_escrow {
                dispatch_cancel_escrow(&env, &call.args)?
            } else if call.function == s_rotate_resolver {
                dispatch_rotate_resolver(&env, &call.args)?
            } else if call.function == s_initialize {
                dispatch_initialize(&env, &call.args)?
            } else if call.function == s_pause_contract {
                dispatch_pause_contract(&env, &call.args)?
            } else if call.function == s_unpause_contract {
                dispatch_unpause_contract(&env, &call.args)?
            } else if call.function == s_get_dispute {
                dispatch_get_dispute(&env, &call.args)?
            } else if call.function == s_get_fee_config {
                dispatch_get_fee_config(&env, &call.args)?
            } else if call.function == s_set_arbitration_fee {
                dispatch_set_arbitration_fee(&env, &call.args)?
            } else if call.function == s_get_arbitration_fee {
                dispatch_get_arbitration_fee(&env, &call.args)?
            } else if call.function == s_create_escrow {
                dispatch_create_escrow(&env, &call.args)?
            } else {
                return Err(ContractError::NotAuthorized);
            };
            results.push_back(res_val);
        }
        Ok(results)
    }"""

helpers = """
fn dispatch_fund_escrow(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let escrow_id: u64 = parse_arg(env, args, 0)?;
    let buyer: Address = parse_arg(env, args, 1)?;
    Escrow::fund_escrow(env.clone(), escrow_id, buyer)?;
    Ok(().into_val(env))
}

fn dispatch_get_escrow(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let escrow_id: u64 = parse_arg(env, args, 0)?;
    let res = Escrow::get_escrow(env.clone(), escrow_id)?;
    Ok(res.into_val(env))
}

fn dispatch_mark_shipped(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    let tracking_id: String = parse_arg(env, args, 2)?;
    Escrow::mark_shipped(env.clone(), caller, escrow_id, tracking_id)?;
    Ok(().into_val(env))
}

fn dispatch_confirm_delivery(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    Escrow::confirm_delivery(env.clone(), caller, escrow_id)?;
    Ok(().into_val(env))
}

fn dispatch_raise_dispute(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    let reason: Symbol = parse_arg(env, args, 2)?;
    let description: String = parse_arg(env, args, 3)?;
    let evidence_hash: BytesN<32> = parse_arg(env, args, 4)?;
    Escrow::raise_dispute(env.clone(), caller, escrow_id, reason, description, evidence_hash)?;
    Ok(().into_val(env))
}

fn dispatch_resolve_dispute(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    let resolution: ResolutionType = parse_arg(env, args, 2)?;
    Escrow::resolve_dispute(env.clone(), caller, escrow_id, resolution)?;
    Ok(().into_val(env))
}

fn dispatch_auto_release(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let escrow_id: u64 = parse_arg(env, args, 0)?;
    Escrow::auto_release(env.clone(), escrow_id)?;
    Ok(().into_val(env))
}

fn dispatch_cancel_escrow(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    Escrow::cancel_escrow(env.clone(), caller, escrow_id)?;
    Ok(().into_val(env))
}

fn dispatch_rotate_resolver(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let escrow_id: u64 = parse_arg(env, args, 1)?;
    let new_resolver: Address = parse_arg(env, args, 2)?;
    Escrow::rotate_resolver(env.clone(), caller, escrow_id, new_resolver)?;
    Ok(().into_val(env))
}

fn dispatch_initialize(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let admin: Address = parse_arg(env, args, 0)?;
    let fee_collector: Address = parse_arg(env, args, 1)?;
    let arbitration_fee_bps: u32 = parse_arg(env, args, 2)?;
    Escrow::initialize(env.clone(), admin, fee_collector, arbitration_fee_bps)?;
    Ok(().into_val(env))
}

fn dispatch_pause_contract(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    Escrow::pause_contract(env.clone(), caller)?;
    Ok(().into_val(env))
}

fn dispatch_unpause_contract(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    Escrow::unpause_contract(env.clone(), caller)?;
    Ok(().into_val(env))
}

fn dispatch_get_dispute(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let escrow_id: u64 = parse_arg(env, args, 0)?;
    let res = Escrow::get_dispute(env.clone(), escrow_id);
    Ok(res.into_val(env))
}

fn dispatch_get_fee_config(env: &Env, _args: &Vec<Val>) -> Result<Val, ContractError> {
    let res = Escrow::get_fee_config(env.clone());
    Ok(res.into_val(env))
}

fn dispatch_set_arbitration_fee(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let caller: Address = parse_arg(env, args, 0)?;
    let fee_bps: u32 = parse_arg(env, args, 1)?;
    Escrow::set_arbitration_fee(env.clone(), caller, fee_bps)?;
    Ok(().into_val(env))
}

fn dispatch_get_arbitration_fee(env: &Env, _args: &Vec<Val>) -> Result<Val, ContractError> {
    let res = Escrow::get_arbitration_fee(env.clone());
    Ok(res.into_val(env))
}

fn dispatch_create_escrow(env: &Env, args: &Vec<Val>) -> Result<Val, ContractError> {
    let payees: Vec<Payee> = parse_arg(env, args, 0)?;
    let buyer: Option<Address> = parse_arg(env, args, 1)?;
    let resolver: Address = parse_arg(env, args, 2)?;
    let token: Address = parse_arg(env, args, 3)?;
    let amount: i128 = parse_arg(env, args, 4)?;
    let fee_bps: u32 = parse_arg(env, args, 5)?;
    let resolver_fee_bps: u32 = parse_arg(env, args, 6)?;
    let shipping_window: u64 = parse_arg(env, args, 7)?;
    let res = Escrow::create_escrow(
        env.clone(),
        payees.into_val(env),
        buyer,
        resolver,
        token,
        amount,
        fee_bps,
        resolver_fee_bps,
        shipping_window,
        None,
    )?;
    Ok(res.into_val(env))
}
"""

final_code = content[:start_idx] + new_multicall + "\n\n" + helper + helpers + content[end_idx:]

with open(r'c:\Users\supre\Documents\trust-link-contract\contracts\escrow\src\instructions.rs.new', 'w', encoding='utf-8') as f:
    f.write(final_code)
print("Done")
