use std::any::TypeId;
use std::convert::TryFrom;

use rhai::{Dynamic, Engine, EvalAltResult, ImmutableString, Scope as RScope};

use polymesh_primitives::{
  investor_zkproof_data::v1::InvestorZKProofData, CddId, Claim, IdentityId, InvestorUid, Scope,
  Ticker,
};

use parity_scale_codec::Encode;

use crate::client::Client;
use crate::types::TypeLookup;
use crate::users::{AccountId, SharedUser};

fn str_to_ticker(val: &str) -> Result<Ticker, Box<EvalAltResult>> {
  let res = if val.len() == 12 {
    Ticker::try_from(val.as_bytes())
  } else {
    let mut ticker = [0u8; 12];
    for (idx, b) in val.as_bytes().iter().take(12).enumerate() {
      ticker[idx] = *b;
    }
    Ticker::try_from(&ticker[..])
  };
  Ok(res.map_err(|e| e.to_string())?)
}

#[derive(Clone)]
pub struct PolymeshUtils {
  client: Client,
}

impl PolymeshUtils {
  pub fn new(client: Client) -> Result<Self, Box<EvalAltResult>> {
    Ok(Self { client })
  }

  pub fn get_did(&self, account_id: AccountId) -> Result<Option<IdentityId>, Box<EvalAltResult>> {
    let key = account_id.encode();
    match self
      .client
      .get_storage_map("Identity", "KeyToIdentityIds", key, None)?
    {
      Some(value) => Ok(Some(value.decode().map_err(|e| e.to_string())?)),
      None => Ok(None),
    }
  }

  pub fn create_investor_uniqueness(
    &mut self,
    mut user: SharedUser,
    ticker: &str,
  ) -> Result<Vec<Dynamic>, Box<EvalAltResult>> {
    let did = self
      .get_did(user.acc())?
      .ok_or_else(|| format!("Missing Identity"))?;
    let uid = InvestorUid::from(confidential_identity_v1::mocked::make_investor_uid(
      did.as_bytes(),
    ));
    let ticker = str_to_ticker(ticker)?;

    let proof = InvestorZKProofData::new(&did, &uid, &ticker);
    let cdd_id = CddId::new_v1(did, uid);

    let scope_id = InvestorZKProofData::make_scope_id(&ticker.as_slice(), &uid);

    let claim = Claim::InvestorUniqueness(Scope::Ticker(ticker), scope_id, cdd_id);
    Ok(vec![Dynamic::from(claim), Dynamic::from(proof)])
  }
}

pub fn init_engine(engine: &mut Engine) {
  engine
    .register_type_with_name::<PolymeshUtils>("PolymeshUtils")
    .register_result_fn(
      "get_did",
      |utils: &mut PolymeshUtils, account_id: AccountId| {
        Ok(match utils.get_did(account_id)? {
          Some(did) => Dynamic::from(did),
          None => Dynamic::UNIT,
        })
      },
    )
    .register_result_fn(
      "create_investor_uniqueness",
      PolymeshUtils::create_investor_uniqueness,
    )
    .register_type_with_name::<Claim>("Claim")
    .register_type_with_name::<InvestorZKProofData>("InvestorZKProofData")
    .register_type_with_name::<IdentityId>("IdentityId")
    .register_type_with_name::<InvestorUid>("InvestorUid")
    .register_type_with_name::<Ticker>("Ticker");
}

pub fn init_scope(
  client: &Client,
  lookup: &TypeLookup,
  _engine: &mut Engine,
  scope: &mut RScope<'_>,
) -> Result<(), Box<EvalAltResult>> {
  let utils = PolymeshUtils::new(client.clone())?;
  scope.push_constant("PolymeshUtils", utils.clone());

  lookup.custom_encode("Signatory", TypeId::of::<SharedUser>(), |value, data| {
    let user = value.cast::<SharedUser>();
    // Encode variant idx.
    data.encode(1u8); // Signatory::Account
    data.encode(user.public());
    Ok(())
  })?;
  lookup.custom_encode(
    "IdentityId",
    TypeId::of::<SharedUser>(),
    move |value, data| {
      let mut user = value.cast::<SharedUser>();
      let did = utils
        .get_did(user.acc())?
        .ok_or_else(|| format!("Missing Identity for user"))?;
      data.encode(did);
      Ok(())
    },
  )?;
  lookup.custom_encode("IdentityId", TypeId::of::<IdentityId>(), |value, data| {
    data.encode(value.cast::<IdentityId>());
    Ok(())
  })?;
  lookup.custom_encode("Ticker", TypeId::of::<ImmutableString>(), |value, data| {
    let value = value.cast::<ImmutableString>();
    let ticker = str_to_ticker(value.as_str())?;
    data.encode(&ticker);
    Ok(())
  })?;
  lookup.custom_encode("Claim", TypeId::of::<Claim>(), |value, data| {
    data.encode(value.cast::<Claim>());
    Ok(())
  })?;
  lookup.custom_encode(
    "InvestorZKProofData",
    TypeId::of::<InvestorZKProofData>(),
    |value, data| {
      data.encode(value.cast::<InvestorZKProofData>());
      Ok(())
    },
  )?;
  Ok(())
}
