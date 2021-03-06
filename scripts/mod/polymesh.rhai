// Wait for all call results and print errors.
fn wait_for_results(results) {
	for res in results {
		if !res.is_success {
			print(`call failed: ${res.result()}`);
		}
	}
}

// Wait for all calls to be included in a block.
// This doesn't check if the call was successful.
fn wait_for_in_block(results) {
	for res in results {
		if !res.is_in_block {
			print(`call failed: Invalid or dropped`);
		}
	}
}

// get multiple user's account ids.
fn get_accounts(users) {
	return users.map(|u| u.acc);
}

// get user's account info.
fn get_account_info(user) {
	return STORAGE.map("System", "Account", user.acc);
}

// get multiple user's account infos.
fn mult_get_account_info(users) {
	let accounts = get_accounts(users);
	return STORAGE.map_keys("System", "Account", accounts);
}

// get user's balance
fn get_balance(user) {
	let info = get_account_info(user);
	if info == () {
		return 0;
	} else {
		return info.data.free;
	}
}

// get multiple user's balance
fn get_balances(users) {
	let infos = mult_get_account_info(users);
	return infos.map(|info| {
		if info == () {
			return 0;
		} else {
			return info.data.free;
		}
	});
}

const TX_VERSION = CLIENT.get_transaction_version();

fn key_records_did(rec) {
	if rec == () {
		return ();
	}
	let p_key = rec.PrimaryKey;
	if p_key != () {
		return p_key;
	}
	let s_key = rec.SecondaryKey;
	if s_key != () {
		return s_key[0];
	}
	return ();
}

// get user's identity id.
fn get_did(user) {
	if global::TX_VERSION < 3 {
		return STORAGE.map("Identity", "KeyToIdentityIds", user.acc);
	} else {
		let rec = STORAGE.map("Identity", "KeyRecords", user.acc);
		return key_records_did(rec);
	}
}

// get multiple user's identity id.
fn get_dids(users) {
	let accounts = get_accounts(users);
	if global::TX_VERSION < 3 {
		return STORAGE.map_keys("Identity", "KeyToIdentityIds", accounts);
	} else {
		return STORAGE.map_keys("Identity", "KeyRecords", accounts)
			.map(|rec| key_records_did(rec));
	}
}

// Mock CDD for user and make sure they have POLYX.
fn mock_user(user, balance) {
	let calls = [];
	let did = get_did(user);
	if did == () {
		calls += TestUtils.mock_cdd_register_did(user);
	}
	calls += Sudo.sudo(Balances.set_balance(user, balance, 0));
	let res = USER.Alice.submit(Utility.batch_optimistic(calls));
	//let res = USER.Eve.submit(Utility.batch_optimistic(calls));
	return res.is_in_block;
}

// Generate list of names from parameters.
fn gen_names(prefix, offset, count) {
	let names = [];
	for idx in range(offset, count) {
		names += `${prefix}${idx}`;
	}

	return names;
}

fn cdd_providers() {
	return [
		USER.Alice,
		USER["Alice//stash"],
		USER.Bob,
		USER.Charlie,
		USER.Dave,
		USER.Eve,
	];
}

// Create a batch of keys.
fn create_keys_batch(prefix, offset, count) {
	return create_keys_batch_with_balance(prefix, offset, count, ());
}

// Create a batch of keys with balance.
fn create_keys_batch_with_balance(prefix, offset, count, balance) {
	print(`CreateKeys: prefix='${prefix}', range=(${offset}..${count}), balance=${balance}`);
	let names = gen_names(prefix, offset, count);

	// Generate keys.
	let keys = [];
	for name in names {
		let key = USER[name];
		keys += key;
	}
	if balance != () {
		// Alice for CDD.
		let alice = USER.Alice;
		//let alice = USER.Eve;

		// Query balances.
		let cur_bals = get_balances(keys);

		print("CreateKeys: Batch 'set_balance' calls");
		let results = [];
		let calls = [];
		for (key, idx) in keys {
			let cur_bal = cur_bals[idx];
			if cur_bal < balance {
				calls += Sudo.sudo(Balances.set_balance(key, balance, 0));
			}
			if calls.len() > 200 {
				results += alice.submit(Utility.batch_optimistic(calls));
				calls = [];
			}
		}

		// Final batch if there are still calls to make.
		if calls.len() > 0 {
			results += alice.submit(Utility.batch_optimistic(calls));
		}
		print("CreateKeys: Wait for batch 'set_balance' results");
		wait_for_in_block(results);
	}
	return keys;
}

// Create a batch of users.
fn create_users_batch(prefix, offset, count, balance) {
	print(`CreateUsers: prefix='${prefix}', range=(${offset}..${count}), balance=${balance}`);
	let names = gen_names(prefix, offset, count);
	return create_users(names, balance);
}

// Create users.
fn create_users(names, balance) {
	let users = [];
	// Alice for CDD.
	let alice = USER.Alice;
	//let alice = USER.Eve;
	
	// Generate users.
	for name in names {
		let user = USER[name];
		users += user;
	}

	// Query dids and balances.
	let dids = get_dids(users);
	let cur_bals = get_balances(users);

	let results = [];
	let calls = [];
	//for name in names {
	//	let user = USER[name];
	//	users += user;
	for (user, idx) in users {
		let did = dids[idx];
		let cur_bal = cur_bals[idx];
		if did == () {
			calls += TestUtils.mock_cdd_register_did(user);
		}
		//print(`user: ${user}, cur: ${cur_bal}, bal: ${balance}`);
		if cur_bal < balance {
			calls += Sudo.sudo(Balances.set_balance(user, balance, 0));
		}
		if calls.len() > 200 {
			results += alice.submit(Utility.batch_optimistic(calls));
			calls = [];
		}
	}

	// Final batch if there are still calls to make.
	if calls.len() > 0 {
		results += alice.submit(Utility.batch_optimistic(calls));
	}
	print("CreateUsers: Wait for batch results");
	wait_for_in_block(results);
	return users;
}

// Create a batch of assets and issuers.
fn create_asset_batch(prefix, offset, count, supply, disable_iu) {
	print(`Create Assets: prefix='${prefix}', range=(${offset}..${count}), supply=${supply}, disable_iu=${disable_iu}`);
	let tickers = gen_names(prefix, offset, count);
	return create_assets(tickers, #{ EquityCommon: () }, supply, disable_iu);
}

// Create a batch of stablecoins and issuers.
fn create_coin_batch(prefix, offset, count, supply) {
	print(`Create Stablecoins: prefix='${prefix}', range=(${offset}..${count}), supply=${supply}`);
	let tickers = gen_names(prefix, offset, count);
	return create_assets(tickers, #{ StableCoin: () }, supply, true);
}

// Create assets.
fn create_assets(tickers, asset_type, supply, disable_iu) {
	let ticker_to_issuer = |ticker| `Issuer_${ticker}`;
	let issuers = create_users(tickers.map(ticker_to_issuer), 10000);

	let results = [];
	for (ticker, idx) in tickers {
		let issuer = issuers[idx];
	
		results += issuer.submit(Utility.batch_optimistic([
			Asset.create_asset(`token ${ticker}`, ticker, true, asset_type, [], (), disable_iu),
			Asset.issue(ticker, supply),
			ComplianceManager.pause_asset_compliance(ticker),
		]));
	}
	// Wait for all results.
	print("CreateAssets: Wait for batch results");
	wait_for_in_block(results);
	
	return [issuers, tickers];
}

// Prepare `Utiltity.relay_tx` call.
fn get_relay_tx_nonce(user) {
	let nonce = STORAGE.map("Utility", "Nonces", user.acc);
	if nonce == () {
		nonce = 0;
	}
	return nonce;
}

// Prepare `Utiltity.relay_tx` call.
fn make_relay_tx(user, c) {
	return make_relay_tx_with_nonce(user, -1, c);
}

fn make_relay_tx_with_nonce(user, nonce, c) {
	let acc = user.acc;
	if nonce < 0 {
		nonce = get_relay_tx_nonce(user);
	}
	let unique_call = #{
		nonce: nonce,
		"call": c,
	};
	let ty = Types.resolve("UniqueCall");
	let data = ty.encode(unique_call);
	let sig = user.sign(data);
	return Utility.relay_tx(acc, sig, unique_call);
}

fn validate_investor_uniqueness_claim(target, claim, proof) {
	let ty_did = Types.resolve("IdentityId");
	let target = ty_did.decode(ty_did.encode(target));
	print(`target = ${target}`);
	let ty_claim = Types.resolve("Claim");
	let claim = ty_claim.decode(ty_claim.encode(claim));
	print(`claim = ${claim}`);
	let ty_proof = Types.resolve("InvestorZKProofData");
	let proof = ty_proof.decode(ty_proof.encode(proof));
	print(`proof = ${proof}`);

	let is_valid = PolymeshUtils.validate_investor_uniqueness(target, claim, proof);
	print(`is_valid = ${is_valid}`);

	return is_valid;
}

fn add_investor_uniqueness_claim(user, ticker) {
	print("add_investor_uniqueness_claim");
	// Create investor uniqueness claim & proof.
	let claim = PolymeshUtils.create_investor_uniqueness(user, "ACME");
	print(`claim = ${claim}`);
	
	// Add Investor Uniqueness claim.
	return user.submit(Identity.add_investor_uniqueness_claim(user, claim[0], claim[1], ()));
}

// get a list of the user's venues.
fn get_user_venues(user) {
	let did = get_did(user);
	return STORAGE.map("Settlement", "UserVenues", did);
}

fn get_or_create_venue(user, venue_type) {
	// Get or create venue.
	let venues = get_user_venues(user);
	let venue_id = ();
	if venues != () && venues.len() > 0 {
		venue_id = venues[0];
	} else {
		print(`Create Venue`);
		let res = user.submit(Settlement.create_venue("", [], venue_type));
		for event in res.events("Settlement.VenueCreated") {
			venue_id = event.args[1];
		}
	}
	return venue_id;
}

fn asset_fund_users(ticker, from, users, amount) {
	// Get or create venue.
	let venue_id = get_or_create_venue(from, #{ Other: () });

	// Create settlement.
	let legs = [];
	let from_did = get_did(from);
	let from_portfolio = #{ did: from_did, kind: #{ Default: () } };
	let portfolios = [];
	for user in users {
		let did = get_did(user);
		let portfolio = #{ did: did, kind: #{ Default: () } };
		legs.push(#{
			from: from_portfolio,
			to: portfolio,
			asset: ticker,
			amount: amount,
		});
		portfolios.push(#{ user: user, portfolio: portfolio });
	}
	let res = from.submit(Settlement.add_and_affirm_instruction(venue_id, #{ SettleOnAffirmation: () }, (), (), legs, [from_portfolio]));
	let instruction_id = ();
	for event in res.events("Settlement.InstructionCreated") {
		instruction_id = event.args[2];
	}

	// Users affirm.
	let results = [];
	for port in portfolios {
		results += port.user.submit(Settlement.affirm_instruction(instruction_id, [port.portfolio], 10));
	}
	wait_for_in_block(results);
}

fn sign_orderbook_order(user, order) {
	// Encode and sign order.
	let order_ty = Types.resolve("Order<AccountId>");
	let encoded_order = order_ty.encode(order);
	let sig = user.sign(encoded_order);

	return #{
		signature: sig,
		order: order,
	};
}
