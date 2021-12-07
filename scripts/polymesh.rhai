fn get_did() {
	return PolymeshUtils.get_did(this.acc);
}

fn mock_user(user, balance) {
	let did = user.get_did();
	if did == () {
		// Mock Cdd for user and make sure they have POLYX.
		let res = USER.Alice.submit(TestUtils.mock_cdd_register_did(user));
		if res.is_success {
			USER.Alice.submit(Balances.transfer(user, balance));
		}
	} else {
		USER.Alice.submit(Balances.transfer(user, balance));
	}
}

fn add_investor_uniqueness_claim(user, ticker) {
	print("add_investor_uniqueness_claim");
	// Create investor uniqueness claim & proof.
	let claim = PolymeshUtils.create_investor_uniqueness(user, "ACME");
	print(`claim = ${claim}`);
	
	// Add Investor Uniqueness claim.
	return user.submit(Identity.add_investor_uniqueness_claim(user, claim[0], claim[1], ()));
}
