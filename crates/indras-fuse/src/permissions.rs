use indras_artifacts::PlayerId;

/// Default permissions for files owned by the player (steward).
pub const OWNER_FILE_PERM: u16 = 0o644;
/// Default permissions for directories owned by the player.
pub const OWNER_DIR_PERM: u16 = 0o755;
/// Read-only permissions for peer-shared artifacts.
pub const PEER_FILE_PERM: u16 = 0o444;
/// Read-only directory permissions for peer views.
pub const PEER_DIR_PERM: u16 = 0o555;
/// Read-only permissions for virtual files.
pub const VIRTUAL_FILE_PERM: u16 = 0o444;
/// Read-only permissions for virtual directories.
pub const VIRTUAL_DIR_PERM: u16 = 0o555;

pub fn file_perm_for_steward(steward: &PlayerId, player: &PlayerId) -> u16 {
    if steward == player {
        OWNER_FILE_PERM
    } else {
        PEER_FILE_PERM
    }
}

pub fn dir_perm_for_steward(steward: &PlayerId, player: &PlayerId) -> u16 {
    if steward == player {
        OWNER_DIR_PERM
    } else {
        PEER_DIR_PERM
    }
}
