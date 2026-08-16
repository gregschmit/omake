SHARED = shared
app: private SECRET = hidden
app: SHARED = overridden
app: dep
	@echo "app: SECRET=[$(SECRET)] SHARED=[$(SHARED)]"
dep:
	@echo "dep: SECRET=[$(SECRET)] SHARED=[$(SHARED)]"
