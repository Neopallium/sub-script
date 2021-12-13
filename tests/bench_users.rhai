print("bench creating Users");

let users = [];
for x in range(0, 6000) {
	let user = USER[`User_${x}`];
	users += user;
	//print(`User[${x}]: ${user}`);
}

print(`Created ${users.len()}`);
