import "mod/polymesh" as p;

print("Get number of secondary keys for an identity.");

// Get arguments.
let name = ARG.shift();
if name == () {
	print("Need user name");
	return;
}

let user = USER[name];
let did = p::get_did(user);
print(`did = ${did}`);

let records = STORAGE.map("Identity", "DidRecords", did);
let primary = records.primary_key;
let keys = records.secondary_keys;
print(`primary_key = ${primary}`);
print(`count secondary_keys = ${keys.len()}`);
