import "mod/polymesh" as p;

print("Create MultiSig");

// Get arguments.
let prefix = ARG.shift();
if prefix == () {
	print("Need prefix for MultiSig");
	return;
}
// Number of signers.
let count = ARG.shift();
count = if count == () { 10 } else { parse_int(count) };
// Required signers.
let sigs_required = ARG.shift();
sigs_required = if sigs_required == () { 3 } else { parse_int(sigs_required) };

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
// Add signer who has an identity.
let signer = USER[`${prefix}_SIG_WITH_DID`];
p::mock_user(signer, 10.0);
signers += signer;

// Create MultiSig
let res = owner.submit(MultiSig.create_multisig(signers, sigs_required));
//print(`res = ${res}`);

// Signers accept auth.
let events = res.events("Identity.AuthorizationAdded");

// Get auth_id from events and accept.
for (event, idx) in events {
	let auth_id = event.args[3];
	let auth = event.args[4];
	let signer = auth.AddMultiSigSigner;
	print(`${event.name}: auth_id=${auth_id}, signer=${signer}`);
	let signer_key = USER.find_by_account(signer);
	let key = signers[idx];
	print(`signer_key=${signer_key}, key=${key}`);
	key.submit(MultiSig.accept_multisig_signer_as_key(auth_id));
}
