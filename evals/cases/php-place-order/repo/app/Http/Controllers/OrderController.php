<?php

namespace App\Http\Controllers;

use App\Http\Requests\PlaceOrderRequest;
use App\Http\Resources\OrderResource;
use App\Models\Order;
use App\Services\OrderService;

class OrderController extends Controller
{
    public function store(PlaceOrderRequest $request, OrderService $orders)
    {
        $order = $orders->place($request->user(), $request->validated());

        return (new OrderResource($order))->response()->setStatusCode(201);
    }

    public function show(Order $order)
    {
        $this->authorize('view', $order);

        return new OrderResource($order->load('items'));
    }
}
