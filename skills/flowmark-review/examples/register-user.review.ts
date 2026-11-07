// flowmark 0.1
/**
 * Register user
 * Public signup. Creates the user and their address in one transaction,
 * then sends a welcome mail through the queue.
 *
 * access:   public (registerUser), system (sendWelcomeMail)
 * input:    SignupInput, raw request !
 * writes:   User, Address (1 transaction)
 * effects:  event UserRegistered → mail WelcomeMail (queued)
 * output:   201 UserResponse
 * shared:   createUser, also used by admin user creation
 * tests:    happy path only
 */

// --- data ---

type SignupInput = { // @ StoreUserRequest.rules
  name: string // required, max 255
  email: Email // unique among users
  password: string // min 8, confirmed
  address: { street: string }
}

type User = { // @ app/Models/User.php
  id: Id
  name: string
  email: Email
  password: Hashed // never serialized
  // ! mass-assignable: a client that reaches User.create can set role=admin @ "protected $fillable"
  role: "user" | "admin" // ? default "user" comes from the migration, not the model
}

type Address = { userId: Id; street: string; zip: string } // @ app/Models/Address.php

type UserResponse = Pick<User, "id" | "name" | "email"> // @ UserResource.toArray

interface Events {
  UserRegistered: { user: User } // @ app/Events/UserRegistered.php
}

// --- flow ---

// tested: @ tests/Feature/SignupTest.php "test_user_can_register"
// ! untested: invalid input, role injection
const registerUser = entry({
  trigger: "POST /api/users", // @ routes/api.php "Route::post('/users'"
  access: "public",
  input: shape<SignupInput>(), // @ StoreUserRequest.rules
  rateLimit: "10/min", // @ routes/api.php "throttle:10,1"
  async handle(req): Promise<UserResponse> { // @ UserController.store
    // ! passes req.raw instead of req.input: role and address.zip reach the DB unvalidated
    const user = await createUser(req.raw) // @ "$request->all()"
    return respond(201, pick(user, "id", "name", "email")) // @ UserResource.toArray
  },
})

// --- details ---

// used by: admin user creation @ AdminUserController.store "createUser("
async function createUser(data: Raw) { // @ UserService.create
  return db.transaction(async () => { // @ "DB::transaction("
    const email = String(data.email).toLowerCase() // L3 @ "Str::lower("
    const user = await db.insert<User>({ ...data, email, password: hash(data.password) }) // @ "User::create("
    await db.insert<Address>({ ...data.address, userId: user.id }) // @ "->address()->create("
    // ! the listener is queued before commit: the mail can go out for a rolled-back user
    emit("UserRegistered", { user }) // @ "event(new UserRegistered"
    return user
  })
}

// --- async ---

const sendWelcomeMail = entry({
  trigger: "event UserRegistered", // @ EventServiceProvider "UserRegistered::class"
  access: "system",
  queued: true, // @ SendWelcomeMail "implements ShouldQueue"
  retries: 3, // @ SendWelcomeMail "$tries = 3"
  handle({ user }) { // @ SendWelcomeMail.handle
    mail(user.email, "WelcomeMail") // ! irreversible: a retry after a timeout can send it twice
  },
})
