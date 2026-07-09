import "./just/build.just"
import "./just/code_check.just"
import "./just/openapi.just"
import "./just/publish.just"
import "./just/run.just"
import "./just/sdk.just"
import "./just/test.just"

SDK_DIR := "sdk"

# Lists all the available commands
default:
    @just --list
