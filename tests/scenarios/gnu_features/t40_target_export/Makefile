app: export APPVAR = from-app
app: helper
	@echo "app sees APPVAR=$(APPVAR)"
helper:
	@printf 'helper env APPVAR=%s\n' "$$APPVAR"
plain:
	@printf 'plain env APPVAR=%s\n' "$$APPVAR"
