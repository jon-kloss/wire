using Microsoft.AspNetCore.Mvc;

namespace SampleApi.Controllers;

[ApiController]
[Route("api/products")]
public class ProductsController : ControllerBase
{
    [HttpGet]
    public IActionResult List() => Ok(new[] { "widget", "gadget" });

    [HttpGet("{id}")]
    public IActionResult GetById(int id) => Ok(new { id });

    [HttpPost]
    public IActionResult Create([FromBody] Product product) => Created($"/api/products/{product.Id}", product);

    [HttpDelete("{id}")]
    public IActionResult Delete(int id) => NoContent();
}

public record Product(int Id, string Name, decimal Price);
