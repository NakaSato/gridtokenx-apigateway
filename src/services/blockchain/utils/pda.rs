use solana_sdk::pubkey::Pubkey;

pub struct PdaUtils;

impl PdaUtils {
    pub fn find_user_account(authority: &Pubkey, program_id: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"user", authority.as_ref()], program_id)
    }

    pub fn find_meter_account(meter_id: &str, program_id: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"meter", meter_id.as_bytes()], program_id)
    }

    pub fn find_token_info(program_id: &Pubkey) -> (Pubkey, u8) {
        Pubkey::find_program_address(&[b"token_info_2022"], program_id)
    }
}
