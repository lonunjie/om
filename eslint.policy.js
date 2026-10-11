// Shared syntax policy (used by eslint.config.js and apps/web/src/lint-policy.test.ts).
export const noInjection = [
  { selector: "JSXAttribute[name.name='dangerouslySetInnerHTML']", message: "No dangerouslySetInnerHTML: messages and themes are data, never HTML." },
  { selector: "MemberExpression[property.name=/^(innerHTML|outerHTML)$/]", message: "No innerHTML/outerHTML: use text nodes or React elements." },
  { selector: "CallExpression[callee.property.name=/^(insertAdjacentHTML|createContextualFragment)$/]", message: "No HTML injection APIs." },
  { selector: "CallExpression[callee.object.name='document'][callee.property.name=/^write(ln)?$/]", message: "No document.write." },
  { selector: "CallExpression[callee.name='eval']", message: "No eval." },
  { selector: "NewExpression[callee.name='Function']", message: "No new Function." },
  { selector: "CallExpression[callee.name=/^(setTimeout|setInterval)$/][arguments.0.type='Literal']", message: "No string timers." },
  { selector: "Literal[value=/^(https?|wss?):\\/\\//]", message: "No hard-coded remote URLs in product code: endpoints come from the user's chosen provider/Home configuration." },
  { selector: "TemplateElement[value.raw=/^(https?|wss?):\\/\\//]", message: "No hard-coded remote URLs in product code." },
];
