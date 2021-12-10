print("Test MultiSig permissions.");

// Use Alice for mocking cdd.
let alice = USER.Alice;

// Test users
let user = USER.Test123;
let signers = [
	USER.Signer1,
	/* Only need one signer.
	USER.Signer2,
	USER.Signer3,
	*/
];

// Mock Cdd for user and make sure they have POLYX.
let res = alice.submit(TestUtils.mock_cdd_register_did(user));
if res.is_success {
	alice.submit(Balances.transfer(user, 50.0));
}

// Create a new multisig.
let res = user.submit(MultiSig.create_multisig(signers, 1));
if !res.is_success {
	print("Failed to create multisig");
}

// Process events to get auth ids and multisig address.
let multisig = ();
for event in res.events {
	if event.name == "Identity.AuthorizationAdded" {
		let acc = event.args[2];
		let auth_id = event.args[3];
		let signer = USER.find_by_account(acc);
		let res2 = signer.submit(MultiSig.accept_multisig_signer_as_key(auth_id));
		if res2.is_success {
			print(" -- Accepted");
		} else {
			print(` -- Accept Failed: ${res2.result}`);
		}
	} else if event.name == "MultiSig.MultiSigCreated" {
		multisig = event.args[1];
		print(`Created multisig: ${multisig}`);
		// Add multisig as signer (secondary key) to user.
		let res2 = user.submit(MultiSig.make_multisig_signer(multisig));
		if !res2.is_success {
			print("Failed to make multisig signer");
		}
	} else {
		//print(`Unhandled event: ${event.name}`);
	}
}

// Try using one signer to create another multisig account.
let signer1 = USER.Signer1;
let new_signers = [
	USER.NewSigner1,
];
print("Try making another multisig using a signer from the first.");
let res = signer1.submit(
	MultiSig.create_or_approve_proposal_as_key(
		multisig,
		MultiSig.create_multisig(new_signers, 1),
		(),
		true
	)
);
if !res.is_success {
	print("Failed to create second multisig");
}

// Process events to get auth ids and multisig address.
for event in res.events {
	if event.name == "MultiSig.ProposalAdded" {
		print(` -- ProposalAdded: ${event.args}`);
	} else if event.name == "MultiSig.ProposalApproved" {
		print(` -- ProposalApproved: ${event.args}`);
	} else if event.name == "Scheduler.Scheduled" {
		print(` -- Schedule: ${event.args}`);
	} else {
		print(`Unhandled event: ${event.name}`);
	}
}

