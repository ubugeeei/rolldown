import { nativePlugin } from 'rolldown/experimental';

export function exampleNativePlugin(config) {
  return nativePlugin('@rolldown/native-plugin-example', { config });
}
