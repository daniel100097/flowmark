<?php

namespace App\Http\Controllers;

use App\Http\Requests\PlaceOrderRequest;
use App\Http\Resources\OrderResource;
use App\Models\Order;
use App\Services\OrderService;
use Illuminate\Support\Facades\Cache;

class OrderController extends Controller
{
    public function store(PlaceOrderRequest $request, OrderService $orders)
    {
        $key = $request->header('Idempotency-Key');
        if ($key && ($orderId = Cache::get("order:{$key}"))) {
            return (new OrderResource(Order::findOrFail($orderId)))->response()->setStatusCode(200);
        }

        $order = $orders->place($request->user(), $request->validated());

        if ($key) {
            Cache::put("order:{$key}", $order->id, now()->addDay());
        }

        return (new OrderResource($order))->response()->setStatusCode(201);
    }

    public function show(Order $order)
    {
        $this->authorize('view', $order);

        return new OrderResource($order->load('items'));
    }
}
