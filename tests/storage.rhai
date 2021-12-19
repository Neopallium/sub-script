import "../scripts/mod/polymesh" as p;

// Test user.
let user = USER.Test123;

// Make sure the user has an Identity.
p::mock_user(user, 20.0);

let val = STORAGE.map("Identity", "KeyToIdentityIds", user.acc);
print(`did = ${val}`);

let paged = STORAGE.double_paged("Identity", "Authorizations", #{
		Account: user.acc
	});
do {
	let val = paged.next();
	print(`auths = ${val}`);
} while paged.has_more;

let paged = STORAGE.double_paged("Identity", "Authorizations", ());
do {
	let val = paged.next();
	print(`auths = ${val}`);
} while paged.has_more;

//*
let paged = STORAGE.map_paged("Identity", "DidRecords");
let total_dids = 0;
let total_keys = 0;
let max_keys = 0;
do {
	let records = paged.next();
	print(`did_records.len = ${records.len()}`);
	for record in records {
		let keys = record.secondary_keys.len();
		print(` -- primary = ${record.primary_key}`);
		print(` -- keys = ${keys}`);
		if keys > max_keys {
			max_keys = keys;
		}
		total_keys += keys;
	}
	//print(`did_records = ${records}`);
	total_dids += records.len();
} while paged.has_more;
print(`total_dids = ${total_dids}`);
print(`total_keys = ${total_keys}`);
print(`max_keys = ${max_keys}`);

let val = STORAGE.map("System", "Account", user.acc);
print(`did = ${val}`);
