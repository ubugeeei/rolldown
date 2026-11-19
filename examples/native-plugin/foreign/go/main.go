package main

/*
#include "bridge.h"
*/
import "C"

import "unsafe"

const outputSource = "export default \"go\";\n"

//export rolldown_go_write_output
func rolldown_go_write_output(
	data *C.uint8_t,
	capacity C.size_t,
	length *C.size_t,
) (status C.uint8_t) {
	status = C.ROLLDOWN_STATUS_ERROR
	defer func() {
		if recover() != nil {
			status = C.ROLLDOWN_STATUS_ERROR
		}
	}()

	if data == nil || length == nil || capacity < C.size_t(len(outputSource)) {
		return C.ROLLDOWN_STATUS_ERROR
	}

	destination := unsafe.Slice((*byte)(unsafe.Pointer(data)), len(outputSource))
	copy(destination, outputSource)
	*length = C.size_t(len(outputSource))
	return C.ROLLDOWN_STATUS_OK
}

func main() {}
