
let user = USER[`TEST_USER`];

print("Put object.");
let res = user.submit(ObjectStore.put([0, 1, 2, 3, 4]));
for event in res.events("") {
	print(`event: ${event}`);
}
print(`result = ${res}`);
