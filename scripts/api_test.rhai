// simple test.

print("Test Client API");

let api = connect("ws://127.0.0.1:9944");
api.print_metadata();

// Test creating users.
let user = USER.Alice;
print(`User: ${user}`);
let user = USER.Bob;
print(`User: ${user}`);

let user = USER.User_1;
print(`User: ${user}`);

// dynamic users.
for x in range(0, 4) {
	let user = USER[`User_${x}`];
	print(`User[${x}]: ${user}`);
}

// dynamic users.
for x in range(0, 4) {
	let user = USER[`User_${x}`];
	print(`User[${x}]: ${user}`);
}

