use soroban_sdk::{Env, IntoVal, Val};

/// Minimum remaining TTL before a persistent entry is refreshed.
pub(crate) const PERSISTENT_TTL_THRESHOLD: u32 = 10_000;
/// Minimum remaining TTL used for frequently-read persistent indexes.
pub(crate) const READ_TTL_THRESHOLD: u32 = 1_000;
/// TTL granted when an active entry is refreshed (approximately 30 days).
pub(crate) const TTL_EXTENSION: u32 = 518_400;

#[inline(always)]
pub(crate) fn refresh_persistent<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, PERSISTENT_TTL_THRESHOLD, TTL_EXTENSION);
}

#[inline(always)]
pub(crate) fn refresh_persistent_read<K>(env: &Env, key: &K)
where
    K: IntoVal<Env, Val>,
{
    env.storage()
        .persistent()
        .extend_ttl(key, READ_TTL_THRESHOLD, TTL_EXTENSION);
}

#[inline(always)]
pub(crate) fn refresh_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(PERSISTENT_TTL_THRESHOLD, TTL_EXTENSION);
}

#[inline(always)]
pub(crate) fn refresh_persistent_if_present<K>(env: &Env, key: &K) -> bool
where
    K: IntoVal<Env, Val> + Clone,
{
    if env.storage().persistent().has(key) {
        refresh_persistent(env, key);
        true
    } else {
        false
    }
}
