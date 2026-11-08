<?php

namespace App\Services;

use App\Events\OrderPlaced;
use App\Models\Order;
use App\Models\User;
use Illuminate\Support\Facades\DB;
use Illuminate\Support\Facades\Log;
use Stripe\StripeClient;

class OrderService
{
    public function __construct(private StripeClient $stripe)
    {
    }

    public function place(User $user, array $data): Order
    {
        return DB::transaction(function () use ($user, $data) {
            $total = 0;
            foreach ($data['items'] as $item) {
                $total += $item['unit_price'] * $item['quantity'];
            }

            $order = Order::create([
                'user_id' => $user->id,
                'status' => 'pending',
                'total' => $total,
                'note' => $data['note'] ?? null,
            ]);

            foreach ($data['items'] as $item) {
                $order->items()->create([
                    'product_id' => $item['product_id'],
                    'quantity' => $item['quantity'],
                    'unit_price' => $item['unit_price'],
                ]);
            }

            $charge = $this->stripe->charges->create([
                'amount' => (int) round($total * 100),
                'currency' => 'eur',
                'customer' => $user->stripe_customer_id,
                'description' => "Order {$order->id}",
            ]);

            $order->update(['status' => 'paid', 'charge_id' => $charge->id]);

            Log::info('order placed', ['order' => $order->id]);

            event(new OrderPlaced($order));

            return $order;
        });
    }
}
