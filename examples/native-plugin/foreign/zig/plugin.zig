const std = @import("std");
const c = @cImport({
    @cInclude("rolldown_plugin.h");
});

const plugin_name = "example:native-zig";
const target_module = "conformance.foreign.js";
const output_source = "export default \"zig\";\n";

fn transform(
    state: ?*anyopaque,
    args: [*c]const c.RolldownTransformArgs,
    output: [*c]c.RolldownTransformOutput,
    error_buffer: [*c]c.RolldownBuffer,
) callconv(.c) u8 {
    _ = state;
    _ = error_buffer;

    if (args == null or output == null or
        args.*.header.abi_version != c.ROLLDOWN_ABI_VERSION or
        args.*.header.size < @sizeOf(c.RolldownTransformArgs) or
        args.*.context.header.abi_version != c.ROLLDOWN_ABI_VERSION or
        args.*.context.header.size < @sizeOf(c.RolldownHookContext) or
        output.*.header.abi_version != c.ROLLDOWN_ABI_VERSION or
        output.*.header.size < @sizeOf(c.RolldownTransformOutput))
    {
        return c.ROLLDOWN_STATUS_INCOMPATIBLE;
    }

    const id = args.*.id;
    if (id.data == null or
        !std.mem.endsWith(u8, id.data[0..id.len], target_module))
    {
        return c.ROLLDOWN_STATUS_OK;
    }

    const host = args.*.context.host;
    if (host == null or
        host.*.header.abi_version != c.ROLLDOWN_ABI_VERSION or
        host.*.header.size < @sizeOf(c.RolldownHostApi))
    {
        return c.ROLLDOWN_STATUS_INCOMPATIBLE;
    }

    const allocate = host.*.buffer_alloc orelse return c.ROLLDOWN_STATUS_INCOMPATIBLE;
    if (allocate(output_source.len, &output.*.code.value) != c.ROLLDOWN_STATUS_OK) {
        return c.ROLLDOWN_STATUS_ERROR;
    }
    if (output.*.code.value.data == null or
        output.*.code.value.len != output_source.len or
        output.*.code.value.capacity < output_source.len)
    {
        return c.ROLLDOWN_STATUS_ERROR;
    }

    @memcpy(output.*.code.value.data[0..output_source.len], output_source);
    output.*.handled = 1;
    output.*.code.present = 1;
    return c.ROLLDOWN_STATUS_OK;
}

export fn rolldown_plugin_init(
    args: [*c]const c.RolldownInitArgs,
    plugin: [*c]c.RolldownPluginDescriptor,
    error_buffer: [*c]c.RolldownBuffer,
) callconv(.c) u8 {
    _ = error_buffer;

    if (args == null or plugin == null or
        args.*.header.abi_version != c.ROLLDOWN_ABI_VERSION or
        args.*.header.size < @sizeOf(c.RolldownInitArgs) or
        args.*.host == null or
        args.*.host.*.header.abi_version != c.ROLLDOWN_ABI_VERSION or
        args.*.host.*.header.size < @sizeOf(c.RolldownHostApi) or
        plugin.*.header.abi_version != c.ROLLDOWN_ABI_VERSION or
        plugin.*.header.size < @sizeOf(c.RolldownPluginDescriptor))
    {
        return c.ROLLDOWN_STATUS_INCOMPATIBLE;
    }

    plugin.* = std.mem.zeroes(c.RolldownPluginDescriptor);
    plugin.*.header = .{
        .abi_version = c.ROLLDOWN_ABI_VERSION,
        .size = @sizeOf(c.RolldownPluginDescriptor),
    };
    plugin.*.name = .{
        .data = plugin_name.ptr,
        .len = plugin_name.len,
    };
    plugin.*.hook_usage = c.ROLLDOWN_HOOK_TRANSFORM;
    plugin.*.transform = transform;
    return c.ROLLDOWN_STATUS_OK;
}
