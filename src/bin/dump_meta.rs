//use sp_keyring::AccountKeyring;
//use sp_core::crypto::Pair;
use sp_core::sr25519::Pair;

use substrate_api_client::{Api};

use anyhow::Result;

fn main() -> Result<()> {
    dotenv::dotenv().ok();
    env_logger::init();

    // instantiate an Api that connects to the given address
    let url = "ws://127.0.0.1:9944";

    // if no signer is set in the whole program, we need to give to Api a specific type instead of an associated type
    // as during compilation the type needs to be defined.
    let api = Api::<Pair>::new(url.into())?;
    // Alice
    //let signer = AccountKeyring::Alice.pair();
    //let api = Api::new(url.into())?.set_signer(signer.clone())?;

    api.metadata.print_overview();

    Ok(())
}
