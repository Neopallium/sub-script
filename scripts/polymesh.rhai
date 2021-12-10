// get user's account info.
fn get_account_info(user) {
	return STORAGE.map("System", "Account", user.acc);
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

// get user's identity id.
fn get_did(user) {
	return STORAGE.map("Identity", "KeyToIdentityIds", user.acc);
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

// Create a batch of users.
fn create_users_batch(prefix, offset, count, balance) {
	print(`CreateUsers: prefix='${prefix}', range=(${offset}..${count}), balance=${balance}`);
	let names = gen_names(prefix, offset, count);
	return create_users(names, balance);
}

// Create users.
fn create_users(names, balance) {
	let users = [];
	// Use Alice for mocking cdd.
	let alice = USER.Alice;
	
	let calls = [];
	for name in names {
		let user = USER[name];
		users += user;
	
		let did = get_did(user);
		if did == () {
			calls += TestUtils.mock_cdd_register_did(user);
		}
		let cur_bal = get_balance(user);
		if cur_bal < balance {
			calls += Sudo.sudo(Balances.set_balance(user, balance, 0));
		}
		if calls.len() > 600 {
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
	let alice = USER.Alice;

	let ticker_to_issuer = |ticker| `Issuer_${ticker}`;
	let issuers = create_users(tickers.map(ticker_to_issuer), 10000);

	let calls = [];
	for (ticker, idx) in tickers {
		let issuer = issuers[idx];
	
		calls += make_relay_tx(issuer, Utility.batch_optimistic([
			Asset.create_asset(`Token ${ticker}`, ticker, true, #{ EquityCommon: () }, [], (), false),
			Asset.issue(ticker, supply),
		]));
		if calls.len() > 200 {
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
fn make_relay_tx(user, c) {
	let acc = user.acc;
	let nonce = STORAGE.map("Utility", "Nonces", acc);
	if nonce == () {
		nonce = 0;
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
