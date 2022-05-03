let key_limit = 100;
let name_limit = 50;
let pallet_limit = 20;
let func_limit = 10;

let total_dids = 0;
let total_keys = 0;
let max_keys = 0;
let max_pallets = 0;
let max_funcs = 0;
let max_name = 0;

let all_pallet_perms = #{};

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
		for key in keys {
			let pallets_len = 0;
			// Get permission type: Whole/These/Except
			let perm_type = key.permissions.extrinsic.keys()[0];
			let pallet_perms = all_pallet_perms[perm_type];
			if pallet_perms == () {
				pallet_perms = #{};
			}

			for pallets in key.permissions.extrinsic.values() {
				if pallets != () {
					let pallet_len = pallets.len();
					if pallet_len > max_pallets {
						max_pallets = pallet_len;
					}
					for pallet in pallets {
						let pallet_name = pallet.pallet_name;
						let name_len = pallet_name.len();
						if name_len > max_name {
							max_name = name_len;
						}
						let extrinsic_perms = pallet_perms[pallet_name];
						if extrinsic_perms == () {
							extrinsic_perms = #{};
						}
						// Get permission type: Whole/These/Except
						let perm_type = pallet.dispatchable_names.keys()[0];
						let used_extrinsics = extrinsic_perms[perm_type];
						if used_extrinsics == () {
							used_extrinsics = #{};
						}
						for funcs in pallet.dispatchable_names.values() {
							if funcs != () {
								let func_len = funcs.len();
								if func_len > max_funcs {
									max_funcs = func_len;
								}
								for func in funcs {
									let used = used_extrinsics[func];
									if used == () {
										used = 0;
									}
									used += 1;
									used_extrinsics[func] = used;
									let name_len = func.len();
									if name_len > max_name {
										max_name = name_len;
									}
								}
							}
						}
						extrinsic_perms[perm_type] = used_extrinsics;
						pallet_perms[pallet_name] = extrinsic_perms;
					}
				}
			}
			all_pallet_perms[perm_type] = pallet_perms;
		}
	}
	total_dids += records.len();
} while paged.has_more;
print(`total_dids = ${total_dids}`);
print(`total_secondary_keys = ${total_keys}`);
print(`max_keys = ${max_keys}`);
print(`max_pallets = ${max_pallets}`);
print(`max_funcs = ${max_funcs}`);

print(`used pallets`);
for pallet_access in all_pallet_perms.keys() {
	print(`pallet Access type: ${pallet_access}`);
	let pallet_perms = all_pallet_perms[pallet_access];
	for pallet_name in pallet_perms.keys() {
		print(`  -- pallet ${pallet_name}`);
		let pallet = pallet_perms[pallet_name];
		for extrinsic_access in pallet.keys() {
			print(`    -- Extrinsic Access type: ${extrinsic_access}`);
			let used_extrinsics = pallet[extrinsic_access];
			for extrinsic in used_extrinsics.keys() {
				//let used = used_extrinsics[extrinsic];
				//print(`      -- ${extrinsic}      # Used ${used}`);
				print(`      -- ${extrinsic}`);
			}
		}
	}
}
