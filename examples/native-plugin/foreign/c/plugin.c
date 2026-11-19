#include "rolldown_plugin.h"

#include <stdbool.h>
#include <string.h>

static const char PLUGIN_NAME[] = "example:native-c";
static const char TARGET_MODULE[] = "conformance.foreign.js";
static const char OUTPUT_SOURCE[] = "export default \"c\";\n";

static bool string_view_ends_with(
  RolldownStringView value,
  const char *suffix,
  size_t suffix_length
) {
  return value.data != NULL && value.len >= suffix_length &&
         memcmp(value.data + value.len - suffix_length, suffix, suffix_length) == 0;
}

static uint8_t transform(
  void *state,
  const RolldownTransformArgs *args,
  RolldownTransformOutput *output,
  RolldownBuffer *error
) {
  (void)state;
  (void)error;

  if (args == NULL || output == NULL || args->header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->header.size < sizeof(*args) ||
      args->context.header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->context.header.size < sizeof(args->context) ||
      output->header.abi_version != ROLLDOWN_ABI_VERSION || output->header.size < sizeof(*output)) {
    return ROLLDOWN_STATUS_INCOMPATIBLE;
  }

  if (!string_view_ends_with(args->id, TARGET_MODULE, sizeof(TARGET_MODULE) - 1)) {
    return ROLLDOWN_STATUS_OK;
  }

  const RolldownHostApi *host = args->context.host;
  if (host == NULL || host->buffer_alloc == NULL ||
      host->header.abi_version != ROLLDOWN_ABI_VERSION || host->header.size < sizeof(*host)) {
    return ROLLDOWN_STATUS_INCOMPATIBLE;
  }

  const size_t length = sizeof(OUTPUT_SOURCE) - 1;
  if (host->buffer_alloc(length, &output->code.value) != ROLLDOWN_STATUS_OK) {
    return ROLLDOWN_STATUS_ERROR;
  }
  if (output->code.value.data == NULL || output->code.value.len != length ||
      output->code.value.capacity < length) {
    return ROLLDOWN_STATUS_ERROR;
  }

  memcpy(output->code.value.data, OUTPUT_SOURCE, length);
  output->handled = 1;
  output->code.present = 1;
  return ROLLDOWN_STATUS_OK;
}

ROLLDOWN_PLUGIN_EXPORT uint8_t rolldown_plugin_init(
  const RolldownInitArgs *args,
  RolldownPluginDescriptor *plugin,
  RolldownBuffer *error
) {
  (void)error;

  if (args == NULL || plugin == NULL || args->header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->header.size < sizeof(*args) || args->host == NULL ||
      args->host->header.abi_version != ROLLDOWN_ABI_VERSION ||
      args->host->header.size < sizeof(*args->host) ||
      plugin->header.abi_version != ROLLDOWN_ABI_VERSION || plugin->header.size < sizeof(*plugin)) {
    return ROLLDOWN_STATUS_INCOMPATIBLE;
  }

  memset(plugin, 0, sizeof(*plugin));
  plugin->header.abi_version = ROLLDOWN_ABI_VERSION;
  plugin->header.size = sizeof(*plugin);
  plugin->name.data = (const uint8_t *)PLUGIN_NAME;
  plugin->name.len = sizeof(PLUGIN_NAME) - 1;
  plugin->hook_usage = ROLLDOWN_HOOK_TRANSFORM;
  plugin->transform = transform;
  return ROLLDOWN_STATUS_OK;
}
