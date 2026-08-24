// Harness/Cordis 的服务端 loader 会优先解包 ESM default。即使本插件的服务端
// 不执行任何业务，也必须提供可挂载的默认插件对象，不能只提供 named exports。
function apply() {}

const plugin = { apply }

export { apply }
export default plugin
