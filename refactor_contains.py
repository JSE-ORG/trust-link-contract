import re

def update_internal():
    with open('contracts/escrow/src/internal.rs', 'r') as f:
        content = f.read()

    # Add the contains helper function
    helper = """pub(crate) fn contains(list: &soroban_sdk::Vec<Address>, target: &Address) -> bool {
    for item in list.iter() {
        if item == *target {
            return true;
        }
    }
    false
}
"""
    # Insert helper before is_token_allowlist_enabled
    content = content.replace('pub(crate) fn is_token_allowlist_enabled', helper + '\npub(crate) fn is_token_allowlist_enabled')

    # Update is_token_allowed
    old_is_token_allowed = """    let allowlist: soroban_sdk::Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::TokenAllowlist)
        .unwrap_or(soroban_sdk::Vec::new(env));
    for allowed_token in allowlist.iter() {
        if allowed_token == *token {
            return Ok(());
        }
    }
    Err(ContractError::TokenNotAllowed)"""
    new_is_token_allowed = """    let allowlist: soroban_sdk::Vec<Address> = env
        .storage()
        .instance()
        .get(&DataKey::TokenAllowlist)
        .unwrap_or(soroban_sdk::Vec::new(env));
    if contains(&allowlist, token) {
        return Ok(());
    }
    Err(ContractError::TokenNotAllowed)"""
    content = content.replace(old_is_token_allowed, new_is_token_allowed)

    # Update validate_resolvers to use contains
    old_validate_resolvers_loop = """        // Ensure all resolvers are unique
        for i in 0..m.resolvers.len() {
            for j in (i + 1)..m.resolvers.len() {
                if m.resolvers.get(i).ok_or(ContractError::IndexOutOfBounds)?
                    == m.resolvers.get(j).ok_or(ContractError::IndexOutOfBounds)?
                {
                    return Err(ContractError::ConflictingRoles);
                }
            }
        }"""
    new_validate_resolvers_loop = """        // Ensure all resolvers are unique
        let mut seen = soroban_sdk::Vec::new(m.resolvers.env());
        for resolver in m.resolvers.iter() {
            if contains(&seen, &resolver) {
                return Err(ContractError::ConflictingRoles);
            }
            seen.push_back(resolver);
        }"""
    content = content.replace(old_validate_resolvers_loop, new_validate_resolvers_loop)

    # Update create_escrow_internal strict resolver check
    old_strict_check = """        let mut found = false;
        for r in approved.iter() {
            if r == resolver {
                found = true;
                break;
            }
        }
        if !found {"""
    new_strict_check = """        if !contains(&approved, &resolver) {"""
    content = content.replace(old_strict_check, new_strict_check)

    with open('contracts/escrow/src/internal.rs', 'w') as f:
        f.write(content)


def update_admin():
    with open('contracts/escrow/src/admin.rs', 'r') as f:
        content = f.read()

    # Update execute_add_approved_resolver
    old_add_app = """        let mut approved: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::ApprovedResolvers).unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        for existing in approved.iter() {
            if existing == resolver {
                return Ok(());
            }
        }
        approved.push_back(resolver.clone());"""
    new_add_app = """        let mut approved: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::ApprovedResolvers).unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        if crate::internal::contains(&approved, &resolver) {
            return Ok(());
        }
        approved.push_back(resolver.clone());"""
    content = content.replace(old_add_app, new_add_app)

    # Update execute_remove_approved_resolver
    old_remove_app = """        let approved: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::ApprovedResolvers).unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        let mut new_approved = soroban_sdk::Vec::new(&env);
        let mut found = false;
        for existing in approved.iter() {
            if existing == resolver {
                found = true;
            } else {
                new_approved.push_back(existing);
            }
        }
        if !found {
            return Err(ContractError::InvalidAddress);
        }"""
    new_remove_app = """        let approved: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::ApprovedResolvers).unwrap_or_else(|| soroban_sdk::Vec::new(&env));
        if !crate::internal::contains(&approved, &resolver) {
            return Err(ContractError::InvalidAddress);
        }
        let mut new_approved = soroban_sdk::Vec::new(&env);
        for existing in approved.iter() {
            if existing != resolver {
                new_approved.push_back(existing);
            }
        }"""
    content = content.replace(old_remove_app, new_remove_app)

    # Update execute_add_allowed_token
    old_add_token = """        let mut allowlist: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::TokenAllowlist).unwrap_or(soroban_sdk::Vec::new(&env));
        for allowed_token in allowlist.iter() {
            if allowed_token == token {
                return Ok(());
            }
        }
        allowlist.push_back(token.clone());"""
    new_add_token = """        let mut allowlist: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::TokenAllowlist).unwrap_or(soroban_sdk::Vec::new(&env));
        if crate::internal::contains(&allowlist, &token) {
            return Ok(());
        }
        allowlist.push_back(token.clone());"""
    content = content.replace(old_add_token, new_add_token)

    # Update execute_remove_allowed_token
    old_remove_token = """        let allowlist: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::TokenAllowlist).unwrap_or(soroban_sdk::Vec::new(&env));
        let mut found = false;
        let mut new_allowlist = soroban_sdk::Vec::new(&env);
        for allowed_token in allowlist.iter() {
            if allowed_token == token {
                found = true;
            } else {
                new_allowlist.push_back(allowed_token);
            }
        }
        if !found {
            return Err(ContractError::TokenNotAllowed);
        }"""
    new_remove_token = """        let allowlist: soroban_sdk::Vec<Address> = env.storage().instance().get(&DataKey::TokenAllowlist).unwrap_or(soroban_sdk::Vec::new(&env));
        if !crate::internal::contains(&allowlist, &token) {
            return Err(ContractError::TokenNotAllowed);
        }
        let mut new_allowlist = soroban_sdk::Vec::new(&env);
        for allowed_token in allowlist.iter() {
            if allowed_token != token {
                new_allowlist.push_back(allowed_token);
            }
        }"""
    content = content.replace(old_remove_token, new_remove_token)

    with open('contracts/escrow/src/admin.rs', 'w') as f:
        f.write(content)


def update_types():
    with open('contracts/escrow/src/types.rs', 'r') as f:
        content = f.read()
        
    old_contains = """            ResolverSet::Multi(m) => {
                for resolver in m.resolvers.clone() {
                    if resolver == *addr {
                        return true;
                    }
                }
                false
            }"""
    new_contains = """            ResolverSet::Multi(m) => crate::internal::contains(&m.resolvers, addr),"""
    content = content.replace(old_contains, new_contains)

    with open('contracts/escrow/src/types.rs', 'w') as f:
        f.write(content)


update_internal()
update_admin()
update_types()
