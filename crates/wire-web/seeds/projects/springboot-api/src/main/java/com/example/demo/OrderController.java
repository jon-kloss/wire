package com.example.demo;

import java.util.List;
import org.springframework.web.bind.annotation.*;

@RestController
@RequestMapping("/api/orders")
public class OrderController {

    @GetMapping
    public List<Order> list() {
        return List.of();
    }

    @GetMapping("/{id}")
    public Order get(@PathVariable Long id) {
        return new Order(id, "pending");
    }

    @PostMapping
    public Order create(@RequestBody Order order) {
        return order;
    }

    @DeleteMapping("/{id}")
    public void delete(@PathVariable Long id) {
    }
}
