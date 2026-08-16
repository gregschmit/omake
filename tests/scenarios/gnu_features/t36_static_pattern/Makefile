objs = a.o b.o
all: $(objs)
$(objs): %.o: %.c
	@echo "$@ from $< (stem $*, all $^)"
