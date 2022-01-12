import "mod/polymesh" as p;

const max_did_budget = 1000000;
const key_limit = 200;
const asset_limit = 200;
const portfolio_limit = 200;
const name_limit = 50;
const pallet_limit = 50;
const extrinsic_limit = 40;

const ticker_type_len = 12;
// `PortfolioKind`: 8 bytes of enum variant + `PortfolioNumber` 8 bytes.
// `PortfolioId`: `IdentityId` 32 bytes + `PortfolioKind` (8 + 8) bytes.
const portfolio_type_len = 32 + 8 + 8;

const min_name_complexity = 10;

fn name_min_complexity(name) {
	let len = name.len();
	if len < global::min_name_complexity {
		len = global::min_name_complexity;
	}
	return len;
}

fn subset_elems(subset) {
	if subset.These != () {
		return subset.These;
	}
	if subset.Except != () {
		return subset.Except;
	}

	return [];
}

fn subset_len(subset) {
	let elems = subset_elems(subset);
	return elems.len();
}

fn subset_complexity(subset) {
	let elems = subset_elems(subset);
	return elems.len();
}

fn pallet_complexity(pallet) {
	let cost = name_min_complexity(pallet.pallet_name);
	let dispatchables = subset_elems(pallet.dispatchable_names);
	for disptachable in dispatchables {
		cost += name_min_complexity(disptachable);
	}
	return cost;
}

fn extrinsics_complexity(extrinsics) {
	let cost = 0;
	let pallets = subset_elems(extrinsics);
	for pallet in pallets {
		cost += pallet_complexity(pallet);
	}
	return cost;
}

fn permission_complexity(perm) {
	let cost = 0;

	cost += subset_complexity(perm.asset) * global::ticker_type_len;
	cost += subset_complexity(perm.portfolio) * global::portfolio_type_len;
	cost += extrinsics_complexity(perm.extrinsic);

	return cost;
}

fn auth_complexity(auth) {
	let perm = auth.JoinIdentity;
	if perm != () {
		// For AuthorizationData::JoinIdentity return the Permissions complexity.
		return permission_complexity(perm);
	} else {
		return 0;
	}
}

// Calculate the complexity of a key's permissions.
fn secondary_key_complexity(key) {
	return permission_complexity(key.permissions);
}

// Calculate the total complexity of permissions in DidRecord.
fn did_record_complexity(record) {
	let complexity = 0;
	for key in record.secondary_keys {
		let key_complexity = secondary_key_complexity(key);
		complexity += key_complexity;
	}

	return complexity;
}

let total_dids = 0;
let total_keys = 0;
let max_keys = 0;
let max_assets = 0;
let max_pallets = 0;
let max_extrinsics = 0;
let max_portfolios = 0;
let max_name = 0;
let max_did_complexity = 0;
let max_permissions_complexity = 0;

let paged = STORAGE.map_paged("Identity", "DidRecords");
paged.set_page_count(500);

do {
	let records = paged.next();
	for record in records {
		let keys = record.secondary_keys;
		let keys_len = keys.len();
		total_keys += keys_len;
		if keys_len > max_keys {
			max_keys = keys_len;
		}
		if keys_len >= key_limit {
			print(` -- primary = ${record.primary_key}`);
			print(` -- secondary keys = ${keys_len}`);
		}
		let did_complexity = did_record_complexity(record);
		if did_complexity > max_did_complexity {
			max_did_complexity = did_complexity;
		}
		for key in keys {
			let perm_complexity = secondary_key_complexity(key);
			if perm_complexity > max_permissions_complexity {
				max_permissions_complexity = perm_complexity;
			}

			let assets_len = subset_len(key.permissions.asset);
			if assets_len > max_assets {
				max_assets = assets_len;
			}
			let portfolios_len = subset_len(key.permissions.portfolio);
			if portfolios_len > max_portfolios {
				max_portfolios = portfolios_len;
			}
			let pallets_len = 0;
			for pallets in key.permissions.extrinsic.values() {
				if pallets != () {
					let pallet_len = pallets.len();
					if pallet_len > max_pallets {
						max_pallets = pallet_len;
					}
					for pallet in pallets {
						let name_len = pallet.pallet_name.len();
						if name_len > max_name {
							max_name = name_len;
						}
						for extrinsics in pallet.dispatchable_names.values() {
							if extrinsics != () {
								let extrinsic_len = extrinsics.len();
								if extrinsic_len > max_extrinsics {
									max_extrinsics = extrinsic_len;
								}
								for func in extrinsics {
									let name_len = func.len();
									if name_len > max_name {
										max_name = name_len;
									}
								}
							}
						}
					}
				}
			}
		}
	}
	total_dids += records.len();
} while paged.has_more;

print(`total_dids = ${total_dids}`);
print(`total_secondary_keys = ${total_keys}`);

print(`max_keys = ${max_keys}`);
print(`max_assets = ${max_assets}`);
print(`max_portfolios = ${max_portfolios}`);

print(`max_pallets = ${max_pallets}`);
print(`max_extrinsics = ${max_extrinsics}`);

print(`max_did_complexity = ${max_did_complexity}`);
print(`max_permissions_complexity = ${max_permissions_complexity}`);

