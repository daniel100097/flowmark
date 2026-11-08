// flowmark 0.1
/**
 * Place order
 * A logged-in user posts line items with their own prices. The order is stored and the card is
 * charged through Stripe inside one DB transaction; a queued listener then mails a confirmation.
 *
 * access:   user (placeOrder), system (sendOrderConfirmation)
 * input:    PlaceOrderInput, unit_price chosen by the client !
 * writes:   Order, OrderItem (1 transaction, the Stripe charge runs inside it)
 * effects:  external Stripe createCharge !, event OrderPlaced → mail OrderConfirmation (queued, 3 attempts)
 * output:   201 OrderResponse
 * tests:    none in the repository
 */

// --- data ---

// `items` has no maximum: one request can carry any number of lines
type PlaceOrderInput = { // @ PlaceOrderRequest.rules
  items: { // at least 1 @ "'items' => ['required', 'array', 'min:1']"
    product_id: number // must exist in products @ "'exists:products,id'"
    quantity: number // integer, 1 to 100 @ "'min:1', 'max:100'"
    // ! the price comes from the client and is never compared with the product's price: a buyer can set any price ≥ 0 and is charged only that
    unit_price: number // @ "'items.*.unit_price' => ['required', 'numeric', 'min:0']"
  }[]
  note?: string | null // max 500 @ "'note' => ['nullable', 'string', 'max:500']"
}

// the logged-in user is an App\Models\User; that model is not in the repository, so these fields come from how they are used
interface Actor {
  email: Email // Mail::to($user) sends to the model's email @ SendOrderConfirmation.handle "Mail::to($event->order->user)"
  // ? whether every user has a Stripe customer is not visible here; without one the charge fails
  stripe_customer_id: string | null // @ OrderService.place "$user->stripe_customer_id"
}

type User = Actor // App\Models\User @ OrderService.place "User $user"

type Order = { // @ app/Models/Order.php
  id: Id
  user_id: Id
  // "pending" is only ever seen inside the transaction: a committed order is "paid"
  status: "pending" | "paid" // @ OrderService.place "'status' => 'pending'"
  total: number // sum of unit_price × quantity from the request
  note: string | null
  charge_id: string | null // Stripe charge id
}

type OrderItem = { order_id: Id; product_id: number; quantity: number; unit_price: number } // @ app/Models/OrderItem.php

// `items` is left out: the relation is not loaded in this flow
// ? wrapped as { data: { ... } } unless JsonResource::withoutWrapping() is called outside this repository
type OrderResponse = Pick<Order, "id" | "status" | "total"> // @ OrderResource.toArray "'items' => $this->whenLoaded('items')"

interface Events {
  // the queued listener gets the order read again from the DB by id
  OrderPlaced: { order: Order } // @ app/Events/OrderPlaced.php "SerializesModels"
}

// --- flow ---

// ! untested: no tests in the repository, neither the charge nor the failure paths
const placeOrder = entry({
  // ? the /api prefix is Laravel's default for routes/api.php; the file that sets it is not in the repository
  trigger: "POST /api/orders", // @ routes/api.php "Route::post('/orders'"
  access: "user", // Sanctum token or session @ routes/api.php "auth:sanctum"
  input: shape<PlaceOrderInput>(), // @ PlaceOrderRequest.rules
  rateLimit: "20/min", // @ routes/api.php "throttle:20,1"
  async handle(req): Promise<OrderResponse> { // @ OrderController.store
    // auth:sanctum guarantees a user
    const order = await storeAndCharge(req.actor!, req.input) // @ "$orders->place($request->user(), $request->validated())"
    return respond(201, pick(order, "id", "status", "total")) // @ "->response()->setStatusCode(201)"
  },
})

// --- details ---

async function storeAndCharge(user: User, input: PlaceOrderInput): Promise<Order> { // @ OrderService.place
  // ! the Stripe charge runs inside the transaction: if anything after it throws (the update, the queue push in event(), the commit), the order rows roll back but the money stays charged, and nothing refunds it
  return db.transaction(async () => { // @ "DB::transaction("
    const total = input.items.reduce((sum, item) => sum + item.unit_price * item.quantity, 0) // L2 @ "$total += $item['unit_price'] * $item['quantity'];"
    const order = await db.insert<Order>({ user_id: user.id, status: "pending", total, note: input.note ?? null }) // @ "Order::create(["
    for (const item of input.items) await db.insert<OrderItem>({ order_id: order.id, ...item }) // @ "$order->items()->create(["
    // ! no idempotency key and no duplicate check: a client that retries after a timeout, or submits twice, gets a second order and a second charge
    // ? a declined card throws a Stripe exception that nothing here catches: the transaction rolls back and the client gets a 500, unless the exception handler (not in the repository) maps it
    const charge = await external("Stripe").createCharge({ // @ "$this->stripe->charges->create(["
      amount: Math.round(total * 100), // cents @ "(int) round($total * 100)"
      currency: "eur", // fixed @ "'currency' => 'eur'"
      customer: user.stripe_customer_id, // no card in the request: the Stripe customer's default card is charged
      description: `Order ${order.id}`,
    })
    // also updates the in-memory order, so the response and the event see "paid"
    await db.update<Order>({ id: order.id }, { status: "paid", charge_id: charge.id }) // @ "$order->update(['status' => 'paid', 'charge_id' => $charge->id]);"
    // !? dispatched before the commit: unless the queue connection sets after_commit (config not in the repository), a worker can take the job before the commit, fail to load the order and fail the job without sending the mail
    emit("OrderPlaced", { order }) // @ "event(new OrderPlaced($order));"
    return order
  })
}

// --- async ---

const sendOrderConfirmation = entry({
  trigger: "event OrderPlaced", // @ EventServiceProvider "OrderPlaced::class => [SendOrderConfirmation::class]"
  access: "system",
  queued: true, // @ SendOrderConfirmation "implements ShouldQueue"
  retries: 2, // $tries counts attempts: 3 attempts = 2 retries @ SendOrderConfirmation "$tries = 3"
  async handle({ order }) { // @ SendOrderConfirmation.handle
    const user = await db.findOrFail<User>({ id: order.user_id }) // L2 the order's user, lazy-loaded @ Order.user "belongsTo(User::class)"
    // ! irreversible: a retry after a timeout can send the mail twice
    mail(user.email, "OrderConfirmation", { order }) // subject "Your order #<id>", view mail.order-confirmation @ "Mail::to($event->order->user)->send(new OrderConfirmation($event->order));" @ OrderConfirmation.build "view('mail.order-confirmation')"
  },
})
