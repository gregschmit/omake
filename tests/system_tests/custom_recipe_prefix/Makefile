all: test1 test2 test3 test4 test5 test6 test7

test1:
	echo "test1"

.RECIPEPREFIX = '>'
test2:
> echo test2

.RECIPEPREFIX =
test3:
	echo "test3"

.RECIPEPREFIX = ">"
test4:
> echo "test4"

.RECIPEPREFIX = "\t"
test5:
	echo "test5"

.RECIPEPREFIX = >
test6:
> echo "test6"

.RECIPEPREFIX = ""
test7:
	echo "test7"