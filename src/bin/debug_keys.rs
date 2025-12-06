use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

fn main() {
    let program_id = Pubkey::from_str("9t3s8sCgVUG9kAgVPsozj8mDpJp9cy6SF5HwRK5nvAHb").unwrap();
    let (market_pda, _) = Pubkey::find_program_address(&[b"market"], &program_id);
    println!("Market PDA: {}", market_pda);

    let wallet = Pubkey::from_str("Esaxsr5tiAKeiL5JBM1E6nQ7ZXyjVGbDMUawnv8VzqXa").unwrap();
    let mint = Pubkey::from_str("7WPEWFhy7V1nW1eqcSCX6mtchnhmSzp2VKYypZiDTnYR").unwrap();
    // SPL Type
    let ata = spl_associated_token_account::get_associated_token_address(&wallet, &mint);
    println!("User ATA (09c...): {}", ata);

    // Check user 2
    let wallet2 = Pubkey::from_str("CEsorv5HznfYsmdDys2S6f3J1aC4EuHba1NgVLX3YWPo").unwrap();
    let ata2 = spl_associated_token_account::get_associated_token_address(&wallet2, &mint);
    println!("User ATA (1fe...): {}", ata2);

    let error_key = "6Ugqk1YGoYmfa9w15kG4H2oy1mRsyosFYDTLTnwR9C6J";
    println!("Error Key: {}", error_key);
}
