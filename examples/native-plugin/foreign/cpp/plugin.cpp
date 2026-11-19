#include "rolldown_plugin.h"

#include <cstring>
#include <string_view>

namespace {

constexpr std::string_view plugin_name = "example:native-cpp";
constexpr std::string_view target_module = "conformance.foreign.js";
constexpr std::string_view output_source = "export default \"cpp\";\n";

bool string_view_ends_with(RolldownStringView value, std::string_view suffix) noexcept {
  if (value.data == nullptr || value.len < suffix.size()) {
    return false;
  }

  const auto *data = reinterpret_cast<const char *>(value.data);
  return std::string_view(data + value.len - suffix.size(), suffix.size()) == suffix;
}

uint8_t transform(
  void *state,
  const RolldownTransformArgs *args,
  RolldownTransformOutput *output,
  RolldownBuffer *error
) noexcept {
  static_cast<void>(state);
  static_cast<void>(error);

  if (args == nullptr || output == nullptr || args->header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->header.size < sizeof(*args) ||
      args->context.header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->context.header.size < sizeof(args->context) ||
      output->header.abi_version != ROLLDOWN_ABI_VERSION || output->header.size < sizeof(*output)) {
    return ROLLDOWN_STATUS_INCOMPATIBLE;
  }

  if (!string_view_ends_with(args->id, target_module)) {
    return ROLLDOWN_STATUS_OK;
  }

  const RolldownHostApi *host = args->context.host;
  if (host == nullptr || host->buffer_alloc == nullptr ||
      host->header.abi_version != ROLLDOWN_ABI_VERSION || host->header.size < sizeof(*host)) {
    return ROLLDOWN_STATUS_INCOMPATIBLE;
  }

  if (host->buffer_alloc(output_source.size(), &output->code.value) != ROLLDOWN_STATUS_OK) {
    return ROLLDOWN_STATUS_ERROR;
  }
  if (output->code.value.data == nullptr || output->code.value.len != output_source.size() ||
      output->code.value.capacity < output_source.size()) {
    return ROLLDOWN_STATUS_ERROR;
  }

  std::memcpy(output->code.value.data, output_source.data(), output_source.size());
  output->handled = 1;
  output->code.present = 1;
  return ROLLDOWN_STATUS_OK;
}

}  // namespace

extern "C" ROLLDOWN_PLUGIN_EXPORT uint8_t rolldown_plugin_init(
  const RolldownInitArgs *args,
  RolldownPluginDescriptor *plugin,
  RolldownBuffer *error
) {
  static_cast<void>(error);

  if (args == nullptr || plugin == nullptr || args->header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->header.size < sizeof(*args) || args->host == nullptr ||
      args->host->header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->host->header.size < sizeof(*args->host) ||
      plugin->header.abi_version != ROLLDOWN_ABI_VERSION || plugin->header.size < sizeof(*plugin)) {
    return ROLLDOWN_STATUS_INCOMPATIBLE;
  }

  std::memset(plugin, 0, sizeof(*plugin));
  plugin->header = {ROLLDOWN_ABI_VERSION, sizeof(*plugin)};
  plugin->name = {
    reinterpret_cast<const uint8_t *>(plugin_name.data()),
    plugin_name.size(),
  };
  plugin->hook_usage = ROLLDOWN_HOOK_TRANSFORM;
  plugin->transform = transform;
  return ROLLDOWN_STATUS_OK;
}
