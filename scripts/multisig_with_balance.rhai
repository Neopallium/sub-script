import "mod/polymesh" as p;

print("MultiSig with balance test.");

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

// non-Sudo: Give the MS some POLYX.  (Fails CDD Check on `ms_account`)
let res1 = USER.Alice.submit(Balances.transfer(#{ Id: ms_account}, 500000.0));
print(`non-sudo res = ${res1}`);

// Sudo: Force the MS account to have a POLYX balance.
let res2 = USER.Alice.submit(Sudo.sudo(Balances.set_balance(#{ Id: ms_account}, 100000.0, 0)));
print(`sudo res = ${res2}`);

// Make a test call using the MS.
let one_signer = signers[0];
let test_user = USER[`${prefix}_TST`];
print(`test_user = ${test_user.acc}`);
p::mock_user(test_user, 0.1);
let call_transfer = Balances.transfer(test_user, 12345.0);
print(`Try transfer. caller=${one_signer}, ms=${ms_account}`);
let ms_res1 = one_signer.submit(MultiSig.create_or_approve_proposal_as_key(ms_account, call_transfer, (), false));
print(`ms_res = ${ms_res1.result}`);

