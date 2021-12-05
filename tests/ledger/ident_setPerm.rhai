print("Test simple transaction with Ledger app");

let user = USER.Test123;

// Polymesh ledger app
//let app = LedgerApps.get_app("Polymesh: tcp:127.0.0.1:9999");
let app = LedgerApps.get_app("Polymesh: HID");
print(`ledger app: ${app}`);

let res = app.submit(Identity.set_permission_to_signer(
	user,
	#{
		asset: #{ Whole: () },
		extrinsic: #{ Whole: () },
		portfolio: #{ Except: [
			#{
				did: "0x5640ceb2611e0a5f9f234253a949263cce0223370a17ac84934c33863670909b",
				kind: #{ Default: () },
			}
		] },
	}
));
print(`call result = ${res}`);
//print(`events = ${res.events}`);
