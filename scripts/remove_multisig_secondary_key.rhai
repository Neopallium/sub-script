import "mod/polymesh" as p;

print("Remove MultiSig Secondary key test.");

// Get arguments.
let prefix = ARG.shift();
if prefix == () {
	print("Need prefix for MultiSig");
	return;
}
// Number of signers.
let count = 2;
// Required signers.
let sigs_required = 1;

// Create MultiSig Owner.
let owner = USER[`${prefix}_OWNER`];
p::mock_user(owner, 1000.0);

// Create signer accounts.
let signers = [];
let names = p::gen_names(`${prefix}_SIGNER_`, 0, count);
for name in names {
	let signer = USER[name];
	signers += signer;
}

// Create MultiSig
let res = owner.submit(MultiSig.create_multisig(signers, sigs_required));
//print(`res = ${res}`);

// Get new MultiSig address.
let events = res.events("MultiSig.MultiSigCreated");
let ms_account = ();
for (event, idx) in events {
	ms_account = event.args[1];
	print(`MultiSig: ${ms_account}`);
}

// Signers accept auth.
let events = res.events("Identity.AuthorizationAdded");

// Get auth_id from events and accept.
for (event, idx) in events {
	let signer = event.args[2];
	let auth_id = event.args[3];
	let auth = event.args[4];
	let ms = auth.AddMultiSigSigner;
	print(`${event.name}: auth_id=${auth_id}, signer=${signer}, ms=${ms}`);
	let signer_key = USER.find_by_account(signer);
	let key = signers[idx];
	print(`signer_key=${signer_key}, key=${key}`);
	key.submit(MultiSig.accept_multisig_signer_as_key(auth_id));
}

// Make MS a secondary key.
print(`Make MultiSig ${ms_account} a secondary key.`);
let res1 = owner.submit(MultiSig.make_multisig_signer(ms_account));

// Transfer some POLYX to MS account.
let res2 = owner.submit(Balances.transfer(#{ Id: ms_account}, 0.0001));

// Give the MS Full permissions.  Just to test making a call.
let ms_signatory = #{ Account: ms_account };
let res3 = owner.submit(Identity.set_permission_to_signer(
	ms_signatory,
	#{
		asset: #{ Whole: () },
		extrinsic: #{ Whole: () },
		portfolio: #{ Whole: () },
	}
));
// Dump all call results.
print(`res = ${res1}`);
print(`res = ${res2}`);
print(`res = ${res3}`);

// Make a test call using the MS.
let one_signer = signers[0];
let test_user = USER[`${prefix}_TST`];
let call_add_auth = Identity.add_authorization(test_user, #{
	RotatePrimaryKey: (),
}, ());
print(`Try adding an authorization. caller=${one_signer}, ms=${ms_account}`);
let ms_res1 = one_signer.submit(MultiSig.create_or_approve_proposal_as_key(ms_account, call_add_auth, (), false));
print(`ms_res = ${ms_res1.result}`);

// Try removing MS secondary key.
let keys = [
	ms_signatory,
];
print(`Remove MS from secondary keys.`);
let res4 = owner.submit(Identity.remove_secondary_keys(keys));

// Retry call using the MS after removing it.
print(`Try adding another authorization.`);
let test2_user = USER[`${prefix}_TST2`];
let call_add_auth2 = Identity.add_authorization(test2_user, #{
	RotatePrimaryKey: (),
}, ());
let ms_res2 = one_signer.submit(MultiSig.create_or_approve_proposal_as_key(ms_account, call_add_auth2, (), false));

print(`res = ${res4.result}`);
print(`ms_res = ${ms_res2.result}`);
