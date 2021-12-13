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

	let last_res;
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
			last_res = alice.submit(Utility.batch_optimistic(calls));
			calls = [];
		}
	}

	// Final batch if there are still calls to make.
	if calls.len() > 0 {
		last_res = alice.submit(Utility.batch_optimistic(calls));
	}
	if last_res != () {
		print("Wait for final batch results");
		print(`res: ${last_res}`);
	}
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

	let calls = [];
	for (ticker, idx) in tickers {
		let issuer = issuers[idx];
	
		calls += make_relay_tx(issuer, Utility.batch_optimistic([
			Asset.create_asset(`Token ${ticker}`, ticker, true, #{ EquityCommon: () }, [], (), false),
			Asset.issue(ticker, supply),
		]));
		if calls.len() > 100 {
			let res = alice.submit(Utility.batch_optimistic(calls));
			print(`res: ${res}`);
			calls = [];
		}
	}
	
	// Final batch if there are still calls to make.
	if calls.len() > 0 {
		let res = alice.submit(Utility.batch_optimistic(calls));
		print(`res: ${res}`);
	}
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
