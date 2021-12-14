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

// get user's identity id.
fn get_did(user) {
	return STORAGE.map("Identity", "KeyToIdentityIds", user.acc);
}

// get multiple user's identity id.
fn get_dids(users) {
	let accounts = get_accounts(users);
	return STORAGE.map_keys("Identity", "KeyToIdentityIds", accounts);
}

// Mock CDD for user and make sure they have POLYX.
fn mock_user(user, balance) {
	let calls = [];
	let did = get_did(user);
	if did == () {
		calls += TestUtils.mock_cdd_register_did(user);
	}
	calls += Sudo.sudo(Balances.set_balance(user, balance, 0));
	USER.Alice.submit(Utility.batch_optimistic(calls));
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
	print(`CreateKeys: prefix='${prefix}', range=(${offset}..${count})`);
	let names = gen_names(prefix, offset, count);

	// Generate keys.
	let keys = [];
	for name in names {
		let key = USER[name];
		keys += key;
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
fn create_asset_batch(prefix, offset, count, supply) {
	print(`CreateAssets: prefix='${prefix}', range=(${offset}..${count}), supply=${supply}`);
	let tickers = gen_names(prefix, offset, count);
	return create_assets(tickers, supply);
}

// Create assets.
fn create_assets(tickers, supply) {
	// Use Alice doing batch calls.
	let alice = USER.Bob; //Alice;

	let ticker_to_issuer = |ticker| `Issuer_${ticker}`;
	let issuers = create_users(tickers.map(ticker_to_issuer), 10000);

	let results = [];
	for (ticker, idx) in tickers {
		let issuer = issuers[idx];
	
		results += issuer.submit(Utility.batch_optimistic([
			Asset.create_asset(`token ${ticker}`, ticker, true, #{ EquityCommon: () }, [], (), false),
			Asset.issue(ticker, supply),
		]));
	}
	// Wait for all results.
	print("CreateAssets: Wait for batch results");
	wait_for_in_block(results);
	
	return issuers;
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

fn add_investor_uniqueness_claim(user, ticker) {
	print("add_investor_uniqueness_claim");
	// Create investor uniqueness claim & proof.
	let claim = PolymeshUtils.create_investor_uniqueness(user, "ACME");
	print(`claim = ${claim}`);
	
	// Add Investor Uniqueness claim.
	return user.submit(Identity.add_investor_uniqueness_claim(user, claim[0], claim[1], ()));
}
