# SPDX-License-Identifier: GPL-3.0-or-later
# Copyright (C) 2026 Vorssaint
#
# Read after CMake's MSVC defaults: their /MD (DLL C runtime) becomes /MT.
foreach(_lang C CXX)
  foreach(_cfg "" _DEBUG _RELEASE _RELWITHDEBINFO _MINSIZEREL)
    if(DEFINED CMAKE_${_lang}_FLAGS${_cfg}_INIT)
      string(REGEX REPLACE "/MDd?" "/MT" CMAKE_${_lang}_FLAGS${_cfg}_INIT "${CMAKE_${_lang}_FLAGS${_cfg}_INIT}")
    endif()
  endforeach()
endforeach()
