// Data builders for editor use. Factory's native compiler parses source and
// NEVER imports this module or executes submitted JS. These grant no authority.
function data(value) {
  if (!value || Array.isArray(value) || typeof value !== 'object') {
    throw new TypeError('Factory builders require a data object');
  }
  return Object.freeze(value);
}
export const defineWorkflow = data;
export const unit = data;
export const barrier = data;
export const nesting = data;
