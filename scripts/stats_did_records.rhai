let key_limit = 100;
let name_limit = 50;
let pallet_limit = 20;
let func_limit = 10;
let asset_limit = 20;
let portfolio_limit = 20;

let total_dids = 0;
let total_keys = 0;
let max_keys = 0;
let max_assets = 0;
let max_pallets = 0;
let max_funcs = 0;
let max_portfolios = 0;
let max_name = 0;

let paged = STORAGE.map_paged("Identity", "DidRecords");
paged.set_page_count(500);

fn perm_len(perm) {
	let len = 0;
	for val in perm.values() {
		if val != () {
			len = val.len();
		}
	}
	return len;
}

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
		for key in keys {
			let assets_len = perm_len(key.permissions.asset);
			if assets_len > max_assets {
				max_assets = assets_len;
			}
			let portfolios_len = perm_len(key.permissions.portfolio);
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
						for funcs in pallet.dispatchable_names.values() {
							if funcs != () {
								let func_len = funcs.len();
								if func_len > max_funcs {
									max_funcs = func_len;
								}
								for func in funcs {
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
print(`max_pallets = ${max_pallets}`);
print(`max_funcs = ${max_funcs}`);
print(`max_portfolios = ${max_portfolios}`);
