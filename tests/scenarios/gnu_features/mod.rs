// Scenarios for GNU-make-specific syntax and extensions: `ifeq`/`ifdef` conditionals, `define`,
// `vpath`, `export`, order-only prerequisites, `.RECIPEPREFIX`, and `%` pattern rules.
mod t18_pattern_rule;
mod t19_pattern_chain;
mod t1_custom_recipe_prefix;
mod t20_conditionals;
mod t22_order_only;
mod t23_export;
mod t26_define;
mod t27_vpath;
mod t28_wildcard;
mod t29_functions;
mod t30_control_functions;
mod t31_function_lines;
mod t32_eval_origin;
mod t33_directives;
mod t34_command_line_vars;
mod t35_path_functions;
mod t36_static_pattern;
mod t37_borrow_recipe;
mod t38_target_vars;
mod t39_intermediate;
