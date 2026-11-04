compose maximum code from these repositories without writing a browser engine from scratch, assign each project to a strict layer in the stack.

## 1. The Unified GPU & Windowing Substrate: Dawn + Sokol/GLFW
Instead of letting ImGui, Skia, and WebGPU each create their own competing Metal/Vulkan/DX12 contexts, standardize on Google Dawn (extracted from wcandillon/react-native-webgpu) as the single native graphics device.

- **Windowing & Input (`imgui-react-runtime`):** Take the windowing/input bootstrap from `tmikov/imgui-react-runtime` (or swap its Sokol context for Dawn's native `wgpu::Surface` swapchain).
- **ImGui as the Browser "Chrome" & DevTools:** Use Dear ImGui (`imgui_impl_wgpu.cpp`) to render the browser's top-level UI: address bar, tab strip, network/bytecode inspector, and performance overlays.
- **Viewport Compositing:** Each browser tab renders into an offscreen `wgpu::Texture`. ImGui displays the active tab simply by passing that `wgpu::TextureView` as an `ImTextureID` into `ImGui::Image()` inside the main window dockspace.

## 2. The JS Engine: `static_h` Dual-Mode Runtime
A single Static Hermes (`shermes`) `HermesRuntime` instance can simultaneously run ahead-of-time compiled native C code and dynamic JSI objects:

- **Static FFI Mode (`$SHBuiltin.extern_c`):** Compile the browser's internal UI (`imgui-unit` and `jslib-unit` from `tmikov/imgui-react-runtime`) ahead-of-time into native code via `shermes`. This drives the ImGui browser shell and event loop with zero JSI boxing overhead.
- **Dynamic JSI Mode (`facebook::jsi::Runtime`):** Because a `static_h` runtime still implements `facebook::jsi::Runtime`, you pass its C++ pointer directly into the initialization functions of Fabric, `react-native-skia`, and `react-native-webgpu` to evaluate untrusted remote scripts and expose Web APIs.

## 3. Layout & DOM: Extracting `ReactCommon` (Fabric) + Yoga
You do not want all of React Native (which pulls in Objective-C/JNI mobile scaffolding). Instead, extract the pure C++ subdirectories from `facebook/react-native/packages/react-native/ReactCommon`:

- **`react/renderer/dom` & `yoga`:** Gives you a W3C-compliant C++ DOM tree (`Node`, `Element`, `Document`, `getBoundingClientRect`), CSS Flexbox/Grid layout calculation via Yoga, and `ReactCommon/react/renderer/core` for event bubbling (`EventTarget`) and shadow tree diffing.
- **Custom Skia Mounting Layer:** In standard React Native, Fabric's "Mounting Phase" translates computed layout frames (`LayoutMetrics`) into iOS `UIView` or Android `android.view.View` instances. In your browser, you replace the platform mounting manager with a lightweight C++ walker that iterates over Fabric's mounted `ShadowView` list (`Rect`, `BorderMetrics`, `SharedColor`, `ParagraphAttributes`) and issues draw commands directly to Skia.

## 4. Painting & Web Graphics: Skia (Graphite) + WebGPU on a Shared Device
Recent versions of `Shopify/react-native-skia` include direct support for Skia Graphite backed by Dawn WebGPU, specifically designed to interoperate with `wcandillon/react-native-webgpu`.

- **DOM & 2D Canvas Painting (`react-native-skia`):** Initialize Skia's `GrDawnGpu` / `skgpu::graphite::Context` using the exact same `wgpu::Device` and `wgpu::Queue` created in Step 1. Your Fabric mounting layer uses Skia's `SkCanvas` and `SkParagraph` (text shaping, font fallback, line breaking) to paint standard DOM boxes, borders, SVGs, and `<canvas getContext("2d")>` into the tab's `wgpu::Texture`.
- **3D & Compute (`react-native-webgpu`):** Expose the generated JSI `HostObject` bindings from `react-native-webgpu` (`navigator.gpu`) to the tab's JS global scope. When a web page renders to a WebGPU canvas context, Dawn writes into a `wgpu::Texture` that Skia composites directly into the page tree with zero CPU readback.

## What You Still Have to Write (The Missing Glue)
While this composition gives you ~80% of a working engine from existing C++ codebases, two browser-specific pieces are not present in any of those repos:

- **HTML & CSS Stylesheet Parser:** Fabric expects trees to be constructed programmatically via JSI calls (e.g., `createNode`, `appendChild`) and styles to be passed as structured Yoga properties. To load raw `.html` and `.css` files, you will need to drop in a C/C++ parser like Lexbor (`lexbor/lexbor`) or GumBo to parse HTML strings and populate the Fabric shadow nodes.
- **Process/Tab Isolation:** In a browser, navigating to a new page or closing a tab must reclaim all DOM nodes and GPU textures. You should instantiate one `HermesRuntime` and one Fabric `SurfaceManager` per tab, while multiplexing them onto the single shared Dawn `wgpu::Device` on the render thread.

| Browser Subsystem | Open-Source Source Repo | Extracted C++ Target / Directory | Binding Mechanism |
| :--- | :--- | :--- | :--- |
| Window, Input & Event Loop | `tmikov/imgui-react-runtime` | `sokol` + `jslib-unit` | Static Hermes `$SHBuiltin.extern_c` |
| Browser Chrome & Dev UI | `tmikov/imgui-react-runtime` | `imgui` + `imgui-unit` | Static Hermes `$SHBuiltin.extern_c` |
| JS Engine & Bytecode Cache | `facebook/hermes` (`static_h`) | `libhermes`, `shermes`, `API/cdp` | C++ Host API + JSI |
| DOM Tree, Events & CSS Layout | `facebook/react-native` | `ReactCommon/react/renderer/dom` + `yoga` | `jsi::HostObject` |
| 2D Layout Painting & Text | `Shopify/react-native-skia` | `cpp/rnskia` + `SkParagraph` (Graphite) | C++ `SkCanvas` + `jsi::HostObject` |
| WebGPU & Native Swapchain | `wcandillon/react-native-webgpu` | Dawn (`libwebgpu_dawn`) + `cpp/` JSI | `jsi::HostObject` (`webgpu.idl`) |

---

To wire Google Dawn, Skia Graphite, Dear ImGui, and Static Hermes (`static_h`) into a single process without context switches or CPU texture copies, all subsystems must reference a single `wgpu::Instance` and `wgpu::Device`.

Because compiling Dawn and Skia Graphite from source within a standard `add_subdirectory` tree takes upwards of 40 minutes and clashes with Hermes's LLVM/Clang flags, the cleanest approach is to consume prebuilt `libwebgpu_dawn` and `libskia` binaries from the `react-native-wgpu` and `react-native-skia` release artifacts (or via a vcpkg/Depot_Tools stage), then compile Hermes (`static_h`), Fabric's C++ core, and your ImGui host together.

```cmake
cmake_minimum_required(VERSION 3.24)
project(HermesBrowser LANGUAGES C CXX OBJC OBJCXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

# 1. Static Hermes (checked out at branch: static_h)
set(HERMES_ENABLE_DEBUGGER ON CACHE BOOL "" FORCE)
add_subdirectory(third_party/hermes)

# 2. Prebuilt Dawn + Skia Graphite (compiled with -DSK_GRAPHITE -DSK_DAWN)
# Point to extracted artifacts from react-native-skia / react-native-wgpu
find_library(DAWN_LIB webgpu_dawn PATHS "${CMAKE_SOURCE_DIR}/prebuilt/lib" REQUIRED)
find_library(SKIA_LIB skia PATHS "${CMAKE_SOURCE_DIR}/prebuilt/lib" REQUIRED)
find_library(SKPARAGRAPH_LIB skparagraph PATHS "${CMAKE_SOURCE_DIR}/prebuilt/lib" REQUIRED)

# 3. Dear ImGui with WebGPU Backend + GLFW
add_subdirectory(third_party/glfw)
add_library(imgui STATIC
    third_party/imgui/imgui.cpp
    third_party/imgui/imgui_draw.cpp
    third_party/imgui/imgui_tables.cpp
    third_party/imgui/imgui_widgets.cpp
    third_party/imgui/backends/imgui_impl_glfw.cpp
    third_party/imgui/backends/imgui_impl_wgpu.cpp
)
target_include_directories(imgui PUBLIC
    third_party/imgui
    third_party/imgui/backends
    prebuilt/include
)
target_compile_definitions(imgui PUBLIC IMGUI_IMPL_WEBGPU_BACKEND_DAWN)
target_link_libraries(imgui PUBLIC glfw ${DAWN_LIB})

# 4. Extracted ReactCommon (Yoga + DOM + JSI bindings) & RN-WebGPU JSI
file(GLOB_RECURSE RN_WEBGPU_CPP "third_party/react-native-wgpu/packages/webgpu/cpp/*.cpp")
add_Executable(hermes_browser
    src/main.cpp
    ${RN_WEBGPU_CPP}
)

target_include_directories(hermes_browser PRIVATE
    prebuilt/include
    prebuilt/include/skia
    third_party/hermes/API
    third_party/hermes/public
    third_party/react-native-wgpu/packages/webgpu/cpp
)

target_link_libraries(hermes_browser PRIVATE
    hermesvm
    jsi
    imgui
    ${DAWN_LIB}
    ${SKIA_LIB}
    ${SKPARAGRAPH_LIB}
)

# Compile Browser Shell UI via shermes ahead-of-time into a C object
add_custom_command(
    OUTPUT "${CMAKE_CURRENT_BINARY_DIR}/shell_ui.o"
    COMMAND $<TARGET_FILE:shermes> -typed -emit-c -O
            "${CMAKE_SOURCE_DIR}/src/js/shell_ui.ts"
            -o "${CMAKE_CURRENT_BINARY_DIR}/shell_ui.c"
    COMMAND ${CMAKE_C_COMPILER} -c "${CMAKE_CURRENT_BINARY_DIR}/shell_ui.c"
            -I"${CMAKE_SOURCE_DIR}/third_party/hermes/include"
            -I"${CMAKE_CURRENT_BINARY_DIR}/third_party/hermes/lib/config"
            -o "${CMAKE_CURRENT_BINARY_DIR}/shell_ui.o"
    DEPENDS shermes "${CMAKE_SOURCE_DIR}/src/js/shell_ui.ts"
)
target_sources(hermes_browser PRIVATE "${CMAKE_CURRENT_BINARY_DIR}/shell_ui.o")
```

## 2. C++ Initialization & Zero-Copy Viewport Sharing (`src/main.cpp`)
The critical invariant in this file is the ownership hierarchy:

- Dawn creates the single `wgpu::Device` and `wgpu::Queue`.
- Skia Graphite wraps that exact device via `skgpu::graphite::DawnBackendContext` and spawns a `Recorder`.
- Each browser tab allocates a shared `wgpu::Texture` with both `RenderAttachment` (so Skia and WebGPU canvases can draw into it) and `TextureBinding` (so ImGui can sample it as an `ImTextureID`).
- Hermes receives the `wgpu::Device` inside `rnwgpu::RNWebGPUManager` to expose `navigator.gpu` over JSI.

```cpp
#include <webgpu/webgpu_cpp.h>
#include <dawn/native/DawnNative.h>
#include <dawn/dawn_proc.h>

// Skia Graphite headers
#include "include/core/SkSurface.h"
#include "include/core/SkColorSpace.h"
#include "include/gpu/graphite/Context.h"
#include "include/gpu/graphite/ContextOptions.h"
#include "include/gpu/graphite/Recorder.h"
#include "include/gpu/graphite/Recording.h"
#include "include/gpu/graphite/Surface.h"
#include "include/gpu/graphite/BackendTexture.h"
#include "include/gpu/graphite/dawn/DawnBackendContext.h"
#include "include/gpu/graphite/dawn/DawnTypes.h"
#include "include/gpu/graphite/dawn/DawnUtils.h"

// ImGui headers
#include "imgui.h"
#include "imgui_impl_glfw.h"
#include "imgui_impl_wgpu.h"
#include <GLFW/glfw3.h>

// Hermes & JSI headers
#include <hermes/hermes.h>
#include <jsi/jsi.h>

using namespace facebook;

struct SharedGpuContext {
    std::unique_ptr<dawn::native::Instance> dawnInstance;
    wgpu::Device device;
    wgpu::Queue queue;
    wgpu::Surface windowSurface;
    wgpu::TextureFormat swapChainFormat = wgpu::TextureFormat::BGRA8Unorm;

    // Skia Graphite state
    std::unique_ptr<skgpu::graphite::Context> skiaContext;
    std::unique_ptr<skgpu::graphite::Recorder> skiaRecorder;
};

struct BrowserTabViewport {
    uint32_t width = 1280;
    uint32_t height = 720;
    wgpu::Texture backingTexture;
    wgpu::TextureView imguiTextureView; // Passed directly to ImGui::Image()
    sk_sp<SkSurface> skiaSurface;       // Target for Fabric / 2D Canvas painting
    std::unique_ptr<jsi::Runtime> hermesRuntime;
};

// ---------------------------------------------------------------------------
// Step 1: Initialize Single Dawn wgpu::Device & Share with Skia Graphite
// ---------------------------------------------------------------------------
SharedGpuContext initSharedGpu(GLFWwindow* window) {
    SharedGpuContext ctx;
    dawnProcSetProcs(&dawn::native::GetProcs());

    wgpu::InstanceDescriptor instanceDesc{};
    instanceDesc.features.timedWaitAnyEnable = true;
    ctx.dawnInstance = std::make_unique<dawn::native::Instance>(&instanceDesc);

    // Request high-performance native adapter (Metal on macOS, Vulkan on Linux)
    wgpu::RequestAdapterOptions adapterOpts{};
    adapterOpts.powerPreference = wgpu::PowerPreference::HighPerformance;
    std::vector<dawn::native::Adapter> adapters = ctx.dawnInstance->EnumerateAdapters(&adapterOpts);
    wgpu::Adapter adapter(adapters[0].Get());

    wgpu::DeviceDescriptor deviceDesc{};
    ctx.device = adapter.CreateDevice(&deviceDesc);
    ctx.queue = ctx.device.GetQueue();

    // Bridge Dawn into Skia Graphite
    skgpu::graphite::DawnBackendContext dawnBackendCtx{};
    dawnBackendCtx.fInstance = wgpu::Instance(ctx.dawnInstance->Get());
    dawnBackendCtx.fDevice = ctx.device;
    dawnBackendCtx.fQueue = ctx.queue;

    skgpu::graphite::ContextOptions skiaOpts{};
    ctx.skiaContext = skgpu::graphite::ContextFactory::MakeDawn(dawnBackendCtx, skiaOpts);
    ctx.skiaRecorder = ctx.skiaContext->makeRecorder();

    return ctx;
}

// ---------------------------------------------------------------------------
// Step 2: Initialize Dear ImGui on the Same Dawn Device
// ---------------------------------------------------------------------------
void initImGui(GLFWwindow* window, SharedGpuContext& gpu) {
    IMGUI_CHECKVERSION();
    ImGui::CreateContext();
    ImGui_ImplGlfw_InitForOther(window, true);

    ImGui_ImplWGPU_InitInfo initInfo{};
    initInfo.Device = gpu.device.Get();
    initInfo.NumFramesInFlight = 3;
    initInfo.RenderTargetFormat = static_cast<WGPUTextureFormat>(gpu.swapChainFormat);
    initInfo.DepthStencilFormat = WGPUTextureFormat_Undefined;
    ImGui_ImplWGPU_Init(&initInfo);
}

// ---------------------------------------------------------------------------
// Step 3: Create Zero-Copy Tab Texture Shared by Skia, WebGPU, and ImGui
// ---------------------------------------------------------------------------
BrowserTabViewport createBrowserTab(SharedGpuContext& gpu, uint32_t width, uint32_t height) {
    BrowserTabViewport tab;
    tab.width = width;
    tab.height = height;

    // 1. Allocate Dawn texture usable as BOTH a render target (Skia/WebGPU) and sampled texture (ImGui)
    wgpu::TextureDescriptor texDesc{};
    texDesc.dimension = wgpu::TextureDimension::e2D;
    texDesc.size = {width, height, 1};
    texDesc.format = wgpu::TextureFormat::RGBA8Unorm;
    texDesc.usage = wgpu::TextureUsage::RenderAttachment |
                    wgpu::TextureUsage::TextureBinding |
                    wgpu::TextureUsage::CopySrc |
                    wgpu::TextureUsage::CopyDst;
    tab.backingTexture = gpu.device.CreateTexture(&texDesc);
    tab.imguiTextureView = tab.backingTexture.CreateView();

    // 2. Wrap the wgpu::Texture into a Skia Graphite BackendTexture & SkSurface
    skgpu::graphite::DawnTextureInfo dawnTexInfo(
        /*sampleCount=*/1,
        skgpu::Mipmapped::kNo,
        wgpu::TextureFormat::RGBA8Unorm,
        texDesc.usage,
        wgpu::TextureAspect::All
    );
    skgpu::graphite::BackendTexture skBackendTex =
        skgpu::graphite::BackendTextures::MakeDawn(tab.backingTexture.Get());

    SkImageInfo imgInfo = SkImageInfo::Make(
        width, height, kRGBA_8888_SkColorType, kPremul_SkAlphaType
    );
    tab.skiaSurface = SkSurfaces::WrapBackendTexture(
        gpu.skiaRecorder.get(), skBackendTex, kRGBA_8888_SkColorType, nullptr, nullptr
    );

    // 3. Spin up isolated Hermes Runtime for the tab and bind navigator.gpu
    auto runtimeConfig = ::hermes::vm::RuntimeConfig::Builder()
        .withES6Proxy(true)
        .withMicrotaskQueue(true)
        .build();
    tab.hermesRuntime = hermes::makeHermesRuntime(runtimeConfig);

    // Expose shared Dawn wgpu::Device into Hermes JSI as `navigator.gpu`
    // (In react-native-wgpu, GPUHostObject wraps the native wgpu::Instance & Device)
    jsi::Object navigator(*tab.hermesRuntime);
    // rnwgpu::installWebGPUBindings(*tab.hermesRuntime, gpu.dawnInstance->Get(), gpu.device);
    tab.hermesRuntime->global().setProperty(*tab.hermesRuntime, "navigator", navigator);

    return tab;
}

// ---------------------------------------------------------------------------
// Step 4: Frame Loop — Paint DOM via Skia, Submit Recording, Composite in ImGui
// ---------------------------------------------------------------------------
void renderFrame(SharedGpuContext& gpu, BrowserTabViewport& tab, wgpu::TextureView backbufferView) {
    // A. Run pending JS callbacks and drain Hermes microtasks
    tab.hermesRuntime->drainMicrotasks();

    // B. Paint Fabric ShadowView layout tree & 2D canvases via Skia into tab.skiaSurface
    SkCanvas* domCanvas = tab.skiaSurface->getCanvas();
    domCanvas->clear(SK_ColorWHITE);
    // fabricMountManager.paintTree(domCanvas);

    // C. Snap Skia Graphite recording and submit to the shared Dawn queue
    std::unique_ptr<skgpu::graphite::Recording> recording = gpu.skiaRecorder->snap();
    if (recording) {
        skgpu::graphite::InsertRecordingInfo insertInfo{};
        insertInfo.fRecording = recording.get();
        gpu.skiaContext->insertRecording(insertInfo);
        gpu.skiaContext->submit(skgpu::graphite::SyncToCpu::kNo);
    }

    // D. Render ImGui Browser Chrome and composite the Tab's wgpu::TextureView
    ImGui_ImplWGPU_NewFrame();
    ImGui_ImplGlfw_NewFrame();
    ImGui::NewFrame();

    ImGui::Begin("Tab 1 - Hermes Browser");
    // Zero-copy: pass the Dawn WGPUView directly as ImTextureID
    ImGui::Image(
        reinterpret_cast<ImTextureID>(tab.imguiTextureView.Get()),
        ImVec2(static_cast<float>(tab.width), static_cast<float>(tab.height))
    );
    ImGui::End();

    ImGui::Render();

    // E. Encode final ImGui pass onto the window swapchain backbuffer
    wgpu::RenderPassColorAttachment colorAttachment{};
    colorAttachment.view = backbufferView;
    colorAttachment.loadOp = wgpu::LoadOp::Clear;
    colorAttachment.storeOp = wgpu::StoreOp::Store;
    colorAttachment.clearValue = {0.1f, 0.1f, 0.1f, 1.0f};

    wgpu::RenderPassDescriptor passDesc{};
    passDesc.colorAttachmentCount = 1;
    passDesc.colorAttachments = &colorAttachment;

    wgpu::CommandEncoder encoder = gpu.device.CreateCommandEncoder();
    wgpu::RenderPassEncoder pass = encoder.BeginRenderPass(&passDesc);
    ImGui_ImplWGPU_RenderDrawData(ImGui::GetDrawData(), pass.Get());
    pass.End();

    wgpu::CommandBuffer commands = encoder.Finish();
    gpu.queue.Submit(1, &commands);
}
```

## Critical Integration Nuances to Watch For
- **Queue Submission Order (`skiaContext->submit()` before `gpu.queue.Submit()`):** Skia Graphite batches its draw passes inside `skgpu::graphite::Recorder` and only encodes actual `wgpu::CommandBuffer` commands onto the Dawn `wgpu::Queue` when `skiaContext->insertRecording()` and `submit()` are called. Always flush the Skia recording before encoding the ImGui render pass that samples `tab.imguiTextureView`, so Dawn's internal barrier tracker sequences the render attachment write before the shader read.
- **Hermes GC Thread vs. Dawn Thread Safety:** Hermes's Hades garbage collector runs finalizers (`~HostObject()`) on a background GC thread. While Dawn's C++ objects (`wgpu::Buffer`, `wgpu::Texture`) are internally reference-counted and thread-safe for release, Skia Graphite's `Recorder` is strictly single-threaded. If a JS `HostObject` holds a `sk_sp<SkSurface>` or `SkImage`, mirror the pattern in `Shopify/react-native-skia/cpp/rnskia/RNSkManager.h` by pushing destructed pointers onto a lock-free queue drained on the main render thread.
- **Resizing Tabs:** When `ImGui::GetContentRegionAvail()` changes on window resize, destroy and recreate `tab.backingTexture`, `tab.imguiTextureView`, and `tab.skiaSurface` before starting the next frame's `ImGui::NewFrame()`, as `ImGui_ImplWGPU_RenderDrawData` holds the raw `WGPUTextureView` pointer until command submission completes.

```cpp
#pragma once

#include <unordered_map>
#include <vector>
#include <memory>

// Fabric Core & Mounting Headers
#include <react/renderer/Scheduler/SchedulerDelegate.h>
#include <react/renderer/mounting/MountingCoordinator.h>
#include <react/renderer/mounting/ShadowViewMutation.h>
#include <react/renderer/components/view/ViewProps.h>
#include <react/renderer/components/text/ParagraphProps.h>
#include <react/renderer/components/text/ParagraphState.h>
#include <react/renderer/graphics/Color.h>

// Skia Core & SkParagraph Headers
#include "include/core/SkCanvas.h"
#include "include/core/SkRRect.h"
#include "include/core/SkPaint.h"
#include "include/core/SkPath.h"
#include "include/core/SkFontMgr.h"
#include "modules/skparagraph/include/Paragraph.h"
#include "modules/skparagraph/include/ParagraphBuilder.h"
#include "modules/skparagraph/include/FontCollection.h"

namespace hermes_browser {

using namespace facebook::react;
namespace skt = skia::textlayout;

struct SkiaMountNode {
    Tag tag{0};
    ComponentHandle componentHandle{0};
    std::string componentName;

    // Retained snapshot of Fabric's computed Yoga Layout & Props
    LayoutMetrics layoutMetrics;
    Props::Shared props;
    State::Shared state;

    // Tree structure ordered by mounting index
    std::vector<Tag> children;

    // Cached SkParagraph layout (rebuilt only on Create/Update mutations)
    std::unique_ptr<skt::Paragraph> cachedParagraph;
    float lastLaidOutWidth{-1.0f};
};

class SkiaMountingManager final : public SchedulerDelegate {
public:
    explicit SkiaMountingManager(sk_sp<SkFontMgr> fontMgr) {
        fontCollection_ = sk_make_sp<skt::FontCollection>();
        fontCollection_->setDefaultFontManager(std::move(fontMgr));
    }

    // -----------------------------------------------------------------------
    // SchedulerDelegate Interface — Called by Fabric when a ShadowTree commits
    // -----------------------------------------------------------------------
    void schedulerDidFinishTransaction(
        const MountingCoordinator::Shared& mountingCoordinator) override {
        auto transaction = mountingCoordinator->pullTransaction();
        if (!transaction.has_value()) {
            return;
        }

        SurfaceId surfaceId = transaction->getSurfaceId();
        const auto& mutations = transaction->getMutations();
        applyMutations(surfaceId, mutations);
    }

    void schedulerDidRequestPreliminaryViewAllocation(
        SurfaceId /*surfaceId*/,
        const ShadowNode& /*shadowNode*/) override {}

    void schedulerDidDispatchCommand(
        const ShadowView& /*shadowView*/,
        const std::string& /*commandName*/,
        const folly::dynamic& /*args*/) override {}

    void schedulerDidSendAccessibilityEvent(
        const ShadowView& /*shadowView*/,
        const std::string& /*eventType*/) override {}

    void schedulerDidSetIsJSResponder(
        const ShadowView& /*shadowView*/,
        bool /*isJSResponder*/,
        bool /*blockNativeResponder*/) override {}

    // -----------------------------------------------------------------------
    // Step 1: Apply Fabric ShadowViewMutations to the Skia Node Arena
    // -----------------------------------------------------------------------
    void applyMutations(SurfaceId surfaceId, const ShadowViewMutationList& mutations) {
        // Ensure root node exists for the SurfaceId (Tag == surfaceId in Fabric)
        if (nodes_.find(surfaceId) == nodes_.end()) {
            SkiaMountNode rootNode;
            rootNode.tag = surfaceId;
            rootNode.componentName = "RootView";
            nodes_.emplace(surfaceId, std::move(rootNode));
        }

        for (const auto& mutation : mutations) {
            switch (mutation.type) {
                case ShadowViewMutation::Create: {
                    const auto& view = mutation.newChildShadowView;
                    SkiaMountNode node;
                    node.tag = view.tag;
                    node.componentHandle = view.componentHandle;
                    node.componentName = view.componentName;
                    node.layoutMetrics = view.layoutMetrics;
                    node.props = view.props;
                    node.state = view.state;

                    if (node.componentName == "Paragraph") {
                        rebuildParagraph(node);
                    }
                    nodes_[view.tag] = std::move(node);
                    break;
                }

                case ShadowViewMutation::Delete: {
                    nodes_.erase(mutation.oldChildShadowView.tag);
                    break;
                }

                case ShadowViewMutation::Insert: {
                    auto& parent = nodes_[mutation.parentShadowView.tag];
                    const auto& childView = mutation.newChildShadowView;
                    // Update child's latest metrics on insert
                    auto& child = nodes_[childView.tag];
                    child.layoutMetrics = childView.layoutMetrics;
                    child.props = childView.props;
                    child.state = childView.state;

                    if (mutation.index >= 0 &&
                        static_cast<size_t>(mutation.index) <= parent.children.size()) {
                        parent.children.insert(
                            parent.children.begin() + mutation.index, childView.tag);
                    } else {
                        parent.children.push_back(childView.tag);
                    }
                    break;
                }

                case ShadowViewMutation::Remove: {
                    auto it = nodes_.find(mutation.parentShadowView.tag);
                    if (it != nodes_.end()) {
                        auto& children = it->second.children;
                        if (mutation.index >= 0 &&
                            static_cast<size_t>(mutation.index) < children.size()) {
                            children.erase(children.begin() + mutation.index);
                        }
                    }
                    break;
                }

                case ShadowViewMutation::Update: {
                    const auto& view = mutation.newChildShadowView;
                    auto& node = nodes_[view.tag];
                    node.layoutMetrics = view.layoutMetrics;
                    node.props = view.props;
                    node.state = view.state;

                    if (node.componentName == "Paragraph") {
                        rebuildParagraph(node);
                    }
                    break;
                }
            }
        }
    }

    // -----------------------------------------------------------------------
    // Step 2: Paint the Mounted Tree onto the Skia Graphite Canvas
    // -----------------------------------------------------------------------
    void paintSurface(SurfaceId surfaceId, SkCanvas* canvas) {
        auto it = nodes_.find(surfaceId);
        if (it == nodes_.end()) {
            return;
        }
        paintNode(it->second, canvas);
    }

private:
    std::unordered_map<Tag, SkiaMountNode> nodes_;
    sk_sp<skt::FontCollection> fontCollection_;

    // Helper: Convert Fabric SharedColor (32-bit ARGB) to SkColor
    static SkColor toSkColor(const SharedColor& sharedColor, SkColor fallback = SK_ColorTRANSPARENT) {
        if (!sharedColor) {
            return fallback;
        }
        ColorComponents c = colorComponentsFromColor(sharedColor);
        return SkColorSetARGB(
            static_cast<U8CPU>(c.alpha * 255.0f),
            static_cast<U8CPU>(c.red * 255.0f),
            static_cast<U8CPU>(c.green * 255.0f),
            static_cast<U8CPU>(c.blue * 255.0f)
        );
    }

    // -----------------------------------------------------------------------
    // Step 3: Build & Shape SkParagraph from Fabric's AttributedString
    // -----------------------------------------------------------------------
    void rebuildParagraph(SkiaMountNode& node) {
        if (!node.state) {
            return;
        }
        auto paragraphState =
            std::static_pointer_cast<const ConcreteState<ParagraphState>>(node.state);
        const AttributedString& attributedString = paragraphState->getData().attributedString;

        skt::ParagraphStyle paraStyle;
        skt::ParagraphBuilderImpl builder(paraStyle, fontCollection_);

        for (const auto& fragment : attributedString.getFragments()) {
            const auto& textAttrs = fragment.textAttributes;

            skt::TextStyle style;
            style.setColor(toSkColor(textAttrs.foregroundColor, SK_ColorBLACK));

            float fontSize = std::isnan(textAttrs.fontSize) ? 14.0f : textAttrs.fontSize;
            style.setFontSize(fontSize);

            if (!textAttrs.fontFamily.empty()) {
                style.setFontFamilies({SkString(textAttrs.fontFamily.c_str())});
            } else {
                style.setFontFamilies({SkString("sans-serif")});
            }

            if (!std::isnan(textAttrs.lineHeight) && fontSize > 0.0f) {
                style.setHeightOverride(true);
                style.setHeight(textAttrs.lineHeight / fontSize);
            }

            builder.pushStyle(style);
            builder.addText(fragment.string.c_str(), fragment.string.size());
            builder.pop();
        }

        node.cachedParagraph = builder.Build();
        float contentWidth = node.layoutMetrics.getContentFrame().size.width;
        node.cachedParagraph->layout(contentWidth > 0.0f ? contentWidth : 10000.0f);
        node.lastLaidOutWidth = contentWidth;
    }

    // -----------------------------------------------------------------------
    // Step 4: Recursive ShadowView Walker (Boxes, Borders, Transforms, Text)
    // -----------------------------------------------------------------------
    void paintNode(SkiaMountNode& node, SkCanvas* canvas) {
        const Rect& frame = node.layoutMetrics.frame;
        if (frame.size.width <= 0.0f && frame.size.height <= 0.0f && node.children.empty()) {
            return;
        }

        const auto* viewProps = dynamic_cast<const ViewProps*>(node.props.get());
        if (viewProps && viewProps->opacity <= 0.0f) {
            return;
        }

        SkAutoCanvasRestore autoRestore(canvas, true);

        // 1. Translate to the node's Yoga frame origin relative to its parent
        canvas->translate(frame.origin.x, frame.origin.y);

        // 2. Apply CSS/View 3D/2D Matrix Transform around the node center
        if (viewProps && viewProps->transform != Transform::Identity()) {
            const float cx = frame.size.width * 0.5f;
            const float cy = frame.size.height * 0.5f;
            const auto& m = viewProps->transform.matrix;
            // Convert Fabric's column-major 4x4 matrix to SkM44
            SkM44 skMatrix(
                m[0], m[4], m[8],  m[12],
                m[1], m[5], m[9],  m[13],
                m[2], m[6], m[10], m[14],
                m[3], m[7], m[11], m[15]
            );
            canvas->translate(cx, cy);
            canvas->concat(skMatrix);
            canvas->translate(-cx, -cy);
        }

        // 3. Apply Layer Opacity if < 1.0
        if (viewProps && viewProps->opacity < 1.0f) {
            SkRect layerBounds = SkRect::MakeWH(frame.size.width, frame.size.height);
            canvas->saveLayerAlpha(&layerBounds, static_cast<U8CPU>(viewProps->opacity * 255.0f));
        }

        // 4. Resolve Border Radii & Border Metrics from Yoga + ViewProps
        SkRect localBounds = SkRect::MakeWH(frame.size.width, frame.size.height);
        SkRRect outerRRect = SkRRect::MakeRect(localBounds);

        if (viewProps) {
            BorderMetrics borderMetrics = viewProps->resolveBorderMetrics(node.layoutMetrics);
            SkVector radii[4] = {
                {borderMetrics.borderRadii.topLeft.horizontal,     borderMetrics.borderRadii.topLeft.vertical},
                {borderMetrics.borderRadii.topRight.horizontal,    borderMetrics.borderRadii.topRight.vertical},
                {borderMetrics.borderRadii.bottomRight.horizontal, borderMetrics.borderRadii.bottomRight.vertical},
                {borderMetrics.borderRadii.bottomLeft.horizontal,  borderMetrics.borderRadii.bottomLeft.vertical}
            };
            outerRRect.setRectRadii(localBounds, radii);

            // 5. Paint Background Color
            SkColor bgColor = toSkColor(viewProps->backgroundColor);
            if (SkColorGetA(bgColor) > 0) {
                SkPaint bgPaint;
                bgPaint.setAntiAlias(true);
                bgPaint.setStyle(SkPaint::kFill_Style);
                bgPaint.setColor(bgColor);
                canvas->drawRRect(outerRRect, bgPaint);
            }

            // 6. Paint Borders (Inset by borderWidths to draw clean inner/outer ring)
            const auto& bw = borderMetrics.borderWidths;
            if (bw.left > 0.0f || bw.top > 0.0f || bw.right > 0.0f || bw.bottom > 0.0f) {
                SkRect innerRect = SkRect::MakeLTRB(
                    bw.left,
                    bw.top,
                    std::max(bw.left, frame.size.width - bw.right),
                    std::max(bw.top, frame.size.height - bw.bottom)
                );
                SkVector innerRadii[4] = {
                    {std::max(0.0f, radii[0].x() - bw.left),  std::max(0.0f, radii[0].y() - bw.top)},
                    {std::max(0.0f, radii[1].x() - bw.right), std::max(0.0f, radii[1].y() - bw.top)},
                    {std::max(0.0f, radii[2].x() - bw.right), std::max(0.0f, radii[2].y() - bw.bottom)},
                    {std::max(0.0f, radii[3].x() - bw.left),  std::max(0.0f, radii[3].y() - bw.bottom)}
                };
                SkRRect innerRRect;
                innerRRect.setRectRadii(innerRect, innerRadii);

                SkPaint borderPaint;
                borderPaint.setAntiAlias(true);
                borderPaint.setStyle(SkPaint::kFill_Style);
                borderPaint.setColor(toSkColor(borderMetrics.borderColors.left, SK_ColorBLACK));
                canvas->drawDRRect(outerRRect, innerRRect, borderPaint);
            }

            // 7. Apply CSS overflow: hidden clipping
            if (viewProps->getClipsContentToBounds()) {
                canvas->clipRRect(outerRRect, SkClipOp::kIntersect, true);
            }
        }

        // 8. If this is a Paragraph leaf node, paint the cached SkParagraph
        if (node.componentName == "Paragraph" && node.cachedParagraph) {
            Rect contentFrame = node.layoutMetrics.getContentFrame();
            if (std::abs(contentFrame.size.width - node.lastLaidOutWidth) > 0.5f) {
                node.cachedParagraph->layout(contentFrame.size.width);
                node.lastLaidOutWidth = contentFrame.size.width;
            }
            node.cachedParagraph->paint(canvas, contentFrame.origin.x, contentFrame.origin.y);
        }

        // 9. Recursively paint child ShadowViews in mounting/z-index order
        for (Tag childTag : node.children) {
            auto childIt = nodes_.find(childTag);
            if (childIt != nodes_.end()) {
                paintNode(childIt->second, canvas);
            }
        }
    }
};

} // namespace hermes_browser
```

## 2. Connecting `TextLayoutManager` so Yoga Measures via `SkParagraph`
A subtle trap when extracting `ReactCommon/react/renderer/components/text` is that Yoga does not know the pixel width or height of a `<p>` or `<span>` node until it calls `ParagraphShadowNode::measureContent()`, which delegates to platform-specific code in `TextLayoutManager.cpp`.

By default, React Native provides stubbed or iOS/Android-specific implementations of `TextLayoutManager::measure()`. In your CMake target, compile your own `TextLayoutManager.cpp` against `SkParagraph` so Yoga's layout pass uses the exact same font metrics that `SkiaMountingManager` paints:

```cpp
// src/fabric_skia/TextLayoutManager.cpp
#include <react/renderer/textlayoutmanager/TextLayoutManager.h>
#include "modules/skparagraph/include/ParagraphBuilder.h"

namespace facebook::react {

TextMeasurement TextLayoutManager::measure(
    const AttributedStringBox& attributedStringBox,
    const ParagraphAttributes& paragraphAttributes,
    const TextLayoutContext& /*layoutContext*/,
    LayoutConstraints layoutConstraints) const {

    const auto& attributedString = attributedStringBox.getValue();

    // Build the SkParagraph using the shared FontCollection stored in nativeTextLayoutManager_
    auto fontCollection = *static_cast<sk_sp<skia::textlayout::FontCollection>*>(
        self_.get());

    skia::textlayout::ParagraphStyle paraStyle;
    if (paragraphAttributes.maximumNumberOfLines > 0) {
        paraStyle.setMaxLines(paragraphAttributes.maximumNumberOfLines);
    }

    skia::textlayout::ParagraphBuilderImpl builder(paraStyle, fontCollection);
    for (const auto& fragment : attributedString.getFragments()) {
        skia::textlayout::TextStyle style;
        style.setFontSize(
            std::isnan(fragment.textAttributes.fontSize) ? 14.0f : fragment.textAttributes.fontSize);
        builder.pushStyle(style);
        builder.addText(fragment.string.c_str(), fragment.string.size());
        builder.pop();
    }

    auto paragraph = builder.Build();
    paragraph->layout(layoutConstraints.maximumSize.width);

    Size measuredSize{
        std::min(paragraph->getMaxIntrinsicWidth(), layoutConstraints.maximumSize.width),
        paragraph->getHeight()
    };

    return TextMeasurement{layoutConstraints.clamp(measuredSize), {}};
}

} // namespace facebook::react
```

## 3. Wiring Hit-Testing & Pointer Events Back into Hermes
Because Fabric retains the `ShadowTree` in C++ alongside Yoga's computed `LayoutMetrics`, you do not need to write your own spatial hit-testing tree for mouse clicks in ImGui:

- In your ImGui frame loop, read the mouse cursor position relative to the `ImGui::Image()` top-left corner (`ImGui::GetMousePos() - ImGui::GetItemRectMin()`).
- Pass that `(x, y)` coordinate into `LayoutableShadowNode::findNodeAtPoint(rootShadowNode, Point{x, y})`.
- Fabric walks the Yoga bounding boxes and `ViewProps::transform` matrices in reverse paint order, returning the exact target `ShadowNode::Shared` and its `EventEmitter`.
- Cast the node's `eventEmitter` to `ViewEventEmitter` and call `viewEventEmitter->onClick(...)` or `onPointerDown(...)`, which dispatches the W3C synthetic event directly into your tab's `HermesRuntime` and queues any resulting React/DOM microtasks.

---

## Pure Rust Host (`winit` + `wgpu` + `vello` + `blitz`) Embedding `libhermes`
If you aren't married to writing the window loop in C++, Blitz already includes a working browser shell (`apps/browser` and `examples/wgpu_texture` in `DioxusLabs/blitz`) built on `winit`, `wgpu`, and Vello (Linebender's compute-shader 2D vector renderer running on WebGPU):

- Blitz already supports compositing custom `wgpu::Texture` surfaces directly inside HTML elements.
- You simply link `libhermes` and `libjsi` into the Rust workspace (either via `cxx` or taking the Hermes Rust bindings from `expo/ibex`), wire `BaseDocument`'s DOM mutator methods to Hermes `HostObject`s, and immediately have a ~12MB standalone multi-platform browser binary with full CSS Grid/Flexbox, WebGPU compute rendering, and Hermes AOT bytecode execution.

## Minimal Rust C-FFI Bridge for `blitz-dom` + `blitz-paint` (`libblitz_bridge`)
Here is what the Rust side looks like to expose `blitz-dom` and `blitz-html` to your C++ Hermes + Skia host:

```rust
// Cargo.toml:
// [lib]
// crate-type = ["staticlib"]
// [dependencies]
// blitz-dom = "0.2"
// blitz-html = "0.2"
// blitz-paint = "0.2"
// blitz-traits = "0.2"
// anyrender = "0.2"

use blitz_dom::{BaseDocument, DocumentConfig};
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use std::ffi::{CStr, c_char};

pub struct BlitzTabDocument {
    pub doc: HtmlDocument,
}

#[no_mangle]
pub unsafe extern "C" fn blitz_doc_create_from_html(
    html_utf8: *const c_char,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut BlitzTabDocument {
    let html_str = CStr::from_ptr(html_utf8).to_str().unwrap_or("");
    let viewport = Viewport::new(width, height, scale, ColorScheme::Dark);

    let doc = HtmlDocument::from_html(
        html_str,
        DocumentConfig {
            viewport: Some(viewport),
            ..Default::default()
        },
    );

    Box::into_raw(Box::new(BlitzTabDocument { doc }))
}

#[no_mangle]
pub unsafe extern "C" fn blitz_doc_set_attribute(
    tab_ptr: *mut BlitzTabDocument,
    node_id: usize,
    attr_name: *const c_char,
    attr_val: *const c_char,
) {
    let tab = &mut *tab_ptr;
    let name = CStr::from_ptr(attr_name).to_str().unwrap_or("");
    let val = CStr::from_ptr(attr_val).to_str().unwrap_or("");

    // DocumentMutator automatically flags Stylo/Taffy damage for incremental relayout
    let mut mutator = tab.doc.mutate();
    mutator.set_attribute(node_id, name, val);
}

#[no_mangle]
pub unsafe extern "C" fn blitz_doc_resolve_layout(tab_ptr: *mut BlitzTabDocument) {
    let tab = &mut *tab_ptr;
    // Runs Stylo cascade diff + Parley inline text shaping + Taffy Flexbox/Grid layout
    tab.doc.as_mut().resolve(0.0);
}
```

To pipe `blitz-paint` directly into your existing C++ `SkCanvas` (backed by Skia Graphite and Dawn), you implement the `anyrender::PaintScene` trait on a lightweight Rust wrapper struct that holds the opaque C++ `SkCanvas*` pointer and forwards draw calls across `extern "C"` callbacks.

Because `anyrender` uses the Linebender ecosystem types (`kurbo` for 2D geometry/paths/transforms and `peniko` for brushes, gradients, fonts, and blend modes), the bridge reduces to two small files:

- A Rust `PaintScene` implementation that decomposes `kurbo::Shape` primitives into flat verb/point arrays and maps Parley/Fontique font blobs (`peniko::FontData`) + positioned glyph runs.
- A C++ receiver (`SkiaBridge.cpp`) that turns those flat arrays into `SkPath`, `SkPaint`, `SkTextBlob`, and `SkCanvas` calls with zero intermediate heap allocations on the hot path.

## 1. Rust Side: Implementing `anyrender::PaintScene` (`src/skia_scene.rs`)
Instead of pulling `rust-skia` into Cargo (which would compile a second, conflicting copy of Skia), your Rust crate declares a small set of `extern "C"` functions implemented in your C++ binary.

To avoid recreating `SkTypeface` on every frame when painting text runs, the bridge caches a mapping from `peniko::Blob::id()` (a unique `u64` assigned by `fontique`/`parley` to each loaded font buffer) to a registered C++ typeface handle.

```rust
use anyrender::{Glyph, NormalizedCoord, Paint, PaintScene};
use kurbo::{Affine, PathEl, Point, Rect, RoundedRect, Shape, Stroke};
use peniko::{BlendMode, BrushRef, Color, Fill, FontData, Image, StyleRef};
use std::collections::HashMap;
use std::ffi::c_void;

// Opaque C++ SkCanvas pointer passed in from main.cpp
#[repr(transparent)]
pub struct CppSkCanvas(*mut c_void);

#[repr(C)]
pub struct CppPathEl {
    pub tag: u8, // 0 = MoveTo, 1 = LineTo, 2 = QuadTo, 3 = CurveTo, 4 = Close
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub x2: f32,
    pub y2: f32,
}

unsafe extern "C" {
    fn skia_bridge_save(canvas: CppSkCanvas);
    fn skia_bridge_restore(canvas: CppSkCanvas);
    fn skia_bridge_set_And_concat_affine(canvas: CppSkCanvas, m: *const [f64; 6]);

    fn skia_bridge_push_clip_rect(
        canvas: CppSkCanvas,
        m: *const [f64; 6],
        x0: f32, y0: f32, x1: f32, y1: f32,
    );
    fn skia_bridge_push_clip_path(
        canvas: CppSkCanvas,
        m: *const [f64; 6],
        els: *const CppPathEl,
        els_len: usize,
        even_odd: bool,
    );
    fn skia_bridge_push_layer(
        canvas: CppSkCanvas,
        alpha: f32,
        m: *const [f64; 6],
        x0: f32, y0: f32, x1: f32, y1: f32,
    );

    fn skia_bridge_draw_rect(
        canvas: CppSkCanvas,
        m: *const [f64; 6],
        x0: f32, y0: f32, x1: f32, y1: f32,
        argb: u32,
        is_stroke: bool,
        stroke_width: f32,
    );
    fn skia_bridge_draw_rrect(
        canvas: CppSkCanvas,
        m: *const [f64; 6],
        x0: f32, y0: f32, x1: f32, y1: f32,
        radii: *const [f32; 4], // tl, tr, br, bl
        argb: u32,
        is_stroke: bool,
        stroke_width: f32,
    );
    fn skia_bridge_draw_path(
        canvas: CppSkCanvas,
        m: *const [f64; 6],
        els: *const CppPathEl,
        els_len: usize,
        argb: u32,
        even_odd: bool,
        is_stroke: bool,
        stroke_width: f32,
    );

    fn skia_bridge_register_font(
        blob_id: u64,
        data: *const u8,
        len: usize,
        ttc_index: u32,
    );
    fn skia_bridge_draw_glyph_run(
        canvas: CppSkCanvas,
        m: *const [f64; 6],
        blob_id: u64,
        font_size: f32,
        argb: u32,
        glyph_ids: *const u16,
        positions_xy: *const f32,
        glyph_count: usize,
    );

    fn skia_bridge_draw_rgba_image(
        canvas: CppSkCanvas,
        m: *const [f64; 6],
        rgba_ptr: *const u8,
        width: u32,
        height: u32,
        alpha: f32,
    );
}

pub struct SkiaBridgeScene {
    canvas: CppSkCanvas,
    path_scratch: Vec<CppPathEl>,
    glyph_id_scratch: Vec<u16>,
    glyph_pos_scratch: Vec<f32>,
    registered_fonts: HashMap<u64, bool>,
}

impl SkiaBridgeScene {
    pub fn new(raw_sk_canvas: *mut c_void) -> Self {
        Self {
            canvas: CppSkCanvas(raw_sk_canvas),
            path_scratch: Vec::with_capacity(128),
            glyph_id_scratch: Vec::with_capacity(256),
            glyph_pos_scratch: Vec::with_capacity(512),
            registered_fonts: HashMap::new(),
        }
    }

    #[inline]
    fn color_to_argb(c: Color, alpha_mul: f32) -> u32 {
        let rgba = c.to_rgba8();
        let a = ((rgba.a as f32) * alpha_mul).clamp(0.0, 255.0) as u32;
        (a << 24) | ((rgba.r as u32) << 16) | ((rgba.g as u32) << 8) | (rgba.b as u32)
    }

    #[inline]
    fn resolve_paint_color(paint: &Paint<'_>) -> u32 {
        match paint {
            Paint::Solid(c) => Self::color_to_argb(*c, 1.0),
            Paint::Gradient(grad) => {
                // Fallback to first stop color or forward stops array for full gradient shader
                grad.stops
                    .first()
                    .map(|s| Self::color_to_argb(s.color.to_alpha_color(), 1.0))
                    .unwrap_or(0xFF000000)
            }
            Paint::Image(_) | Paint::Custom(_) => 0xFF000000,
        }
    }

    fn flatten_shape_to_scratch(&mut self, shape: &impl Shape) {
        self.path_scratch.clear();
        for el in shape.path_elements(0.1) {
            match el {
                PathEl::MoveTo(p) => self.path_scratch.push(CppPathEl {
                    tag: 0, x0: p.x as f32, y0: p.y as f32,
                    x1: 0.0, y1: 0.0, x2: 0.0, y2: 0.0,
                }),
                PathEl::LineTo(p) => self.path_scratch.push(CppPathEl {
                    tag: 1, x0: p.x as f32, y0: p.y as f32,
                    x1: 0.0, y1: 0.0, x2: 0.0, y2: 0.0,
                }),
                PathEl::QuadTo(p1, p2) => self.path_scratch.push(CppPathEl {
                    tag: 2, x0: p1.x as f32, y0: p1.y as f32,
                    x1: p2.x as f32, y1: p2.y as f32, x2: 0.0, y2: 0.0,
                }),
                PathEl::CurveTo(p1, p2, p3) => self.path_scratch.push(CppPathEl {
                    tag: 3, x0: p1.x as f32, y0: p1.y as f32,
                    x1: p2.x as f32, y1: p2.y as f32, x2: p3.x as f32, y2: p3.y as f32,
                }),
                PathEl::ClosePath => self.path_scratch.push(CppPathEl {
                    tag: 4, x0: 0.0, y0: 0.0, x1: 0.0, y1: 0.0, x2: 0.0, y2: 0.0,
                }),
            }
        }
    }
}

impl PaintScene for SkiaBridgeScene {
    fn reset(&mut self) {
        self.path_scratch.clear();
    }

    fn push_layer(
        &mut self,
        _blend: impl Into<BlendMode>,
        alpha: f32,
        transform: Affine,
        clip: &impl Shape,
    ) {
        let coeffs = transform.as_coeffs();
        if let Some(rect) = clip.as_rect() {
            unsafe {
                if (alpha - 1.0).abs() < 1e-4 {
                    skia_bridge_push_clip_rect(
                        self.canvas,
                        &coeffs,
                        rect.x0 as f32, rect.y0 as f32,
                        rect.x1 as f32, rect.y1 as f32,
                    );
                } else {
                    skia_bridge_push_layer(
                        self.canvas,
                        alpha,
                        &coeffs,
                        rect.x0 as f32, rect.y0 as f32,
                        rect.x1 as f32, rect.y1 as f32,
                    );
                }
            }
            return;
        }

        self.flatten_shape_to_scratch(clip);
        unsafe {
            skia_bridge_push_clip_path(
                self.canvas,
                &coeffs,
                self.path_scratch.as_ptr(),
                self.path_scratch.len(),
                false,
            );
        }
    }

    fn pop_layer(&mut self) {
        unsafe {
            skia_bridge_restore(self.canvas);
        }
    }

    fn fill<'a>(
        &mut self,
        style: Fill,
        transform: Affine,
        paint: impl Into<Paint<'a>>,
        _brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        let paint_ref: Paint<'a> = paint.into();
        let argb = Self::resolve_paint_color(&paint_ref);
        let coeffs = transform.as_coeffs();

        // Fast path 1: Axis-aligned Rectangle
        if let Some(rect) = shape.as_rect() {
            unsafe {
                skia_bridge_draw_rect(
                    self.canvas, &coeffs,
                    rect.x0 as f32, rect.y0 as f32, rect.x1 as f32, rect.y1 as f32,
                    argb, false, 0.0,
                );
            }
            return;
        }

        // Fast path 2: CSS Rounded Rectangle (border-radius)
        if let Some(rrect) = shape.as_rounded_rect() {
            let rect = rrect.rect();
            let r = rrect.radii();
            let radii = [
                r.top_left as f32,
                r.top_right as f32,
                r.bottom_right as f32,
                r.bottom_left as f32,
            ];
            unsafe {
                skia_bridge_draw_rrect(
                    self.canvas, &coeffs,
                    rect.x0 as f32, rect.y0 as f32, rect.x1 as f32, rect.y1 as f32,
                    &radii, argb, false, 0.0,
                );
            }
            return;
        }

        // General path fallback (borders, SVGs, arbitrary Bezier curves)
        self.flatten_shape_to_scratch(shape);
        let even_odd = matches!(style, Fill::EvenOdd);
        unsafe {
            skia_bridge_draw_path(
                self.canvas,
                &coeffs,
                self.path_scratch.as_ptr(),
                self.path_scratch.len(),
                argb,
                even_odd,
                false,
                0.0,
            );
        }
    }

    fn stroke<'a>(
        &mut self,
        style: &Stroke,
        transform: Affine,
        paint: impl Into<Paint<'a>>,
        _brush_transform: Option<Affine>,
        shape: &impl Shape,
    ) {
        let paint_ref: Paint<'a> = paint.into();
        let argb = Self::resolve_paint_color(&paint_ref);
        let coeffs = transform.as_coeffs();
        let width = style.width as f32;

        if let Some(rect) = shape.as_rect() {
            unsafe {
                skia_bridge_draw_rect(
                    self.canvas, &coeffs,
                    rect.x0 as f32, rect.y0 as f32, rect.x1 as f32, rect.y1 as f32,
                    argb, true, width,
                );
            }
            return;
        }

        self.flatten_shape_to_scratch(shape);
        unsafe {
            skia_bridge_draw_path(
                self.canvas,
                &coeffs,
                self.path_scratch.as_ptr(),
                self.path_scratch.len(),
                argb,
                false,
                true,
                width,
            );
        }
    }

    fn draw_glyphs<'a, 's: 'a>(
        &mut self,
        font: &'a FontData,
        font_size: f32,
        _hint: bool,
        _normalized_coords: &'a [NormalizedCoord],
        _style: impl Into<StyleRef<'a>>,
        paint: impl Into<Paint<'a>>,
        alpha: f32,
        transform: Affine,
        glyph_transform: Option<Affine>,
        glyphs: impl Iterator<Item = Glyph>,
    ) {
        let blob_id = font.data.id();
        if !self.registered_fonts.contains_key(&blob_id) {
            let bytes = font.data.as_ref();
            unsafe {
                skia_bridge_register_font(blob_id, bytes.as_ptr(), bytes.len(), font.index);
            }
            self.registered_fonts.insert(blob_id, true);
        }

        self.glyph_id_scratch.clear();
        self.glyph_pos_scratch.clear();

        for g in glyphs {
            self.glyph_id_scratch.push(g.id as u16);
            self.glyph_pos_scratch.push(g.x);
            self.glyph_pos_scratch.push(g.y);
        }

        if self.glyph_id_scratch.is_empty() {
            return;
        }

        let combined_xfm = match glyph_transform {
            Some(gt) => transform * gt,
            None => transform,
        };
        let coeffs = combined_xfm.as_coeffs();
        let paint_ref: Paint<'a> = paint.into();
        let base_argb = Self::resolve_paint_color(&paint_ref);
        // Apply glyph run alpha multiplier
        let a = (((base_argb >> 24) & 0xFF) as f32 * alpha).clamp(0.0, 255.0) as u32;
        let argb = (base_argb & 0x00FFFFFF) | (a << 24);

        unsafe {
            skia_bridge_draw_glyph_run(
                self.canvas,
                &coeffs,
                blob_id,
                font_size,
                argb,
                self.glyph_id_scratch.as_ptr(),
                self.glyph_pos_scratch.as_ptr(),
                self.glyph_id_scratch.len(),
            );
        }
    }

    fn draw_image(&mut self, image: &Image, transform: Affine) {
        let coeffs = transform.as_coeffs();
        let bytes = image.data.as_ref();
        unsafe {
            skia_bridge_draw_rgba_image(
                self.canvas,
                &coeffs,
                bytes.as_ptr(),
                image.width,
                image.height,
                1.0,
            );
        }
    }
}
```

And the exported entry point in `lib.rs` that invokes `blitz_paint::paint_scene`:

```rust
#[no_mangle]
pub unsafe extern "C" fn blitz_doc_paint_to_skia(
    tab_ptr: *mut BlitzTabDocument,
    raw_sk_canvas: *mut c_void,
    width: u32,
    height: u32,
    scale: f64,
) {
    let tab = &mut *tab_ptr;
    // 1. Resolve Stylo cascade, Parley inline text layout, and Taffy Flexbox/Grid
    tab.doc.as_mut().resolve(0.0);

    // 2. Stream draw commands through SkiaBridgeScene onto the C++ SkCanvas
    let mut scene = SkiaBridgeScene::new(raw_sk_canvas);
    blitz_paint::paint_scene(&mut scene, tab.doc.as_ref(), scale, width, height);
}
```

## 2. C++ Side: Implementing the `SkCanvas` Receiver (`src/SkiaBridge.cpp`)
On the C++ side, `kurbo::Affine::as_coeffs()` returns `[a, b, c, d, e, f]` representing the 2D affine matrix:

$$\begin{bmatrix} a & c & e \\ b & d & f \\ 0 & 0 & 1 \end{bmatrix}$$

which maps directly to `SkMatrix::MakeAll(a, c, e, b, d, f, 0, 0, 1)`.

For text runs, Parley has already performed full HarfBuzz-level shaping, font fallback, bidirectional reordering, and line breaking in Rust—providing exact `(glyph_id, x, y)` pairs. We load the raw font bytes into an `SkTypeface` via `SkFontMgr::makeFromData` once per `blob_id`, and emit an `SkTextBlob` using `allocRunPos`.

```cpp
#include "include/core/SkCanvas.h"
#include "include/core/SkPath.h"
#include "include/core/SkPaint.h"
#include "include/core/SkRRect.h"
#include "include/core/SkFont.h"
#include "include/core/SkFontMgr.h"
#include "include/core/SkTextBlob.h"
#include "include/core/SkData.h"
#include "include/core/SkImage.h"
#include "include/core/SkSurface.h"

#include <unordered_map>
#include <cstdint>

namespace {

// Global or thread-local typeface cache keyed by Fontique/Peniko Blob ID
std::unordered_map<uint64_t, sk_sp<SkTypeface>> gTypefaceCache;
sk_sp<SkFontMgr> gDefaultFontMgr; // Initialized at startup in main.cpp

inline SkMatrix toSkMatrix(const double m[6]) {
    return SkMatrix::MakeAll(
        static_cast<SkScalar>(m[0]), static_cast<SkScalar>(m[2]), static_cast<SkScalar>(m[4]),
        static_cast<SkScalar>(m[1]), static_cast<SkScalar>(m[3]), static_cast<SkScalar>(m[5]),
        0.0f, 0.0f, 1.0f
    );
}

struct CppPathEl {
    uint8_t tag;
    float x0, y0, x1, y1, x2, y2;
};

inline SkPath buildSkPath(const CppPathEl* els, size_t len, bool evenOdd) {
    SkPath path;
    path.setFillType(evenOdd ? SkPathFillType::kEvenOdd : SkPathFillType::kWinding);
    for (size_t i = 0; i < len; ++i) {
        const auto& e = els[i];
        switch (e.tag) {
            case 0: path.moveTo(e.x0, e.y0); break;
            case 1: path.lineTo(e.x0, e.y0); break;
            case 2: path.quadTo(e.x0, e.y0, e.x1, e.y1); break;
            case 3: path.cubicTo(e.x0, e.y0, e.x1, e.y1, e.x2, e.y2); break;
            case 4: path.close(); break;
        }
    }
    return path;
}

} // namespace

extern "C" {

void skia_bridge_restore(void* canvasPtr) {
    static_cast<SkCanvas*>(canvasPtr)->restore();
}

void skia_bridge_push_clip_rect(
    void* canvasPtr, const double m[6],
    float x0, float y0, float x1, float y1) {
    auto* canvas = static_cast<SkCanvas*>(canvasPtr);
    canvas->save();
    canvas->concat(toSkMatrix(m));
    canvas->clipRect(SkRect::MakeLTRB(x0, y0, x1, y1), SkClipOp::kIntersect, true);
}

void skia_bridge_push_clip_path(
    void* canvasPtr, const double m[6],
    const CppPathEl* els, size_t elsLen, bool evenOdd) {
    auto* canvas = static_cast<SkCanvas*>(canvasPtr);
    canvas->save();
    canvas->concat(toSkMatrix(m));
    SkPath path = buildSkPath(els, elsLen, evenOdd);
    canvas->clipPath(path, SkClipOp::kIntersect, true);
}

void skia_bridge_push_layer(
    void* canvasPtr, float alpha, const double m[6],
    float x0, float y0, float x1, float y1) {
    auto* canvas = static_cast<SkCanvas*>(canvasPtr);
    SkRect bounds = SkRect::MakeLTRB(x0, y0, x1, y1);
    // saveLayerAlpha pushes a single state frame on the SkCanvas stack,
    // so pop_layer()'s single canvas->restore() balances it cleanly.
    SkPaint layerPaint;
    layerPaint.setAlpha(static_cast<U8CPU>(alpha * 255.0f));
    SkMatrix skM = toSkMatrix(m);
    SkRect mappedBounds = skM.mapRect(bounds);
    canvas->saveLayer(&mappedBounds, &layerPaint);
    canvas->concat(skM);
    canvas->clipRect(bounds, SkClipOp::kIntersect, true);
}

void skia_bridge_draw_rect(
    void* canvasPtr, const double m[6],
    float x0, float y0, float x1, float y1,
    uint32_t argb, bool isStroke, float strokeWidth) {
    auto* canvas = static_cast<SkCanvas*>(canvasPtr);
    SkAutoCanvasRestore acr(canvas, true);
    canvas->concat(toSkMatrix(m));

    SkPaint paint;
    paint.setAntiAlias(true);
    paint.setColor(argb);
    paint.setStyle(isStroke ? SkPaint::kStroke_Style : SkPaint::kFill_Style);
    if (isStroke) paint.setStrokeWidth(strokeWidth);

    canvas->drawRect(SkRect::MakeLTRB(x0, y0, x1, y1), paint);
}

void skia_bridge_draw_rrect(
    void* canvasPtr, const double m[6],
    float x0, float y0, float x1, float y1,
    const float radii[4], uint32_t argb, bool isStroke, float strokeWidth) {
    auto* canvas = static_cast<SkCanvas*>(canvasPtr);
    SkAutoCanvasRestore acr(canvas, true);
    canvas->concat(toSkMatrix(m));

    SkVector skRadii[4] = {
        {radii[0], radii[0]},
        {radii[1], radii[1]},
        {radii[2], radii[2]},
        {radii[3], radii[3]}
    };
    SkRRect rrect;
    rrect.setRectRadii(SkRect::MakeLTRB(x0, y0, x1, y1), skRadii);

    SkPaint paint;
    paint.setAntiAlias(true);
    paint.setColor(argb);
    paint.setStyle(isStroke ? SkPaint::kStroke_Style : SkPaint::kFill_Style);
    if (isStroke) paint.setStrokeWidth(strokeWidth);

    canvas->drawRRect(rrect, paint);
}

void skia_bridge_draw_path(
    void* canvasPtr, const double m[6],
    const CppPathEl* els, size_t elsLen,
    uint32_t argb, bool evenOdd, bool isStroke, float strokeWidth) {
    auto* canvas = static_cast<SkCanvas*>(canvasPtr);
    SkAutoCanvasRestore acr(canvas, true);
    canvas->concat(toSkMatrix(m));

    SkPath path = buildSkPath(els, elsLen, evenOdd);
    SkPaint paint;
    paint.setAntiAlias(true);
    paint.setColor(argb);
    paint.setStyle(isStroke ? SkPaint::kStroke_Style : SkPaint::kFill_Style);
    if (isStroke) paint.setStrokeWidth(strokeWidth);

    canvas->drawPath(path, paint);
}

void skia_bridge_register_font(
    uint64_t blobId, const uint8_t* data, size_t len, uint32_t ttcIndex) {
    if (!gDefaultFontMgr) return;
    sk_sp<SkData> skData = SkData::MakeWithCopy(data, len);
    sk_sp<SkTypeface> typeface = gDefaultFontMgr->makeFromData(
        std::move(skData), static_cast<int>(ttcIndex));
    if (typeface) {
        gTypefaceCache[blobId] = std::move(typeface);
    }
}

void skia_bridge_draw_glyph_run(
    void* canvasPtr, const double m[6],
    uint64_t blobId, float fontSize, uint32_t argb,
    const uint16_t* glyphIds, const float* positionsXY, size_t glyphCount) {
    auto it = gTypefaceCache.find(blobId);
    if (it == gTypefaceCache.end() || glyphCount == 0) return;

    auto* canvas = static_cast<SkCanvas*>(canvasPtr);
    SkAutoCanvasRestore acr(canvas, true);
    canvas->concat(toSkMatrix(m));

    SkFont font(it->second, fontSize);
    font.setEdging(SkFont::Edging::kSubpixelAntiAlias);
    font.setSubpixel(true);

    SkTextBlobBuilder builder;
    const auto& runBuffer = builder.allocRunPos(font, static_cast<int>(glyphCount));
    std::memcpy(runBuffer.glyphs, glyphIds, glyphCount * sizeof(uint16_t));
    std::memcpy(runBuffer.pos, positionsXY, glyphCount * 2 * sizeof(float));

    SkPaint paint;
    paint.setAntiAlias(true);
    paint.setColor(argb);

    canvas->drawTextBlob(builder.make(), 0.0f, 0.0f, paint);
}

void skia_bridge_draw_rgba_image(
    void* canvasPtr, const double m[6],
    const uint8_t* rgbaPtr, uint32_t width, uint32_t height, float alpha) {
    auto* canvas = static_cast<SkCanvas*>(canvasPtr);
    SkAutoCanvasRestore acr(canvas, true);
    canvas->concat(toSkMatrix(m));

    SkImageInfo info = SkImageInfo::Make(
        static_cast<int>(width), static_cast<int>(height),
        kRGBA_8888_SkColorType, kUnpremul_SkAlphaType);
    SkPixmap pixmap(info, rgbaPtr, width * 4);
    sk_sp<SkImage> img = SkImages::RasterFromPixmap(pixmap, nullptr, nullptr);
    if (img) {
        SkPaint paint;
        paint.setAlpha(static_cast<U8CPU>(alpha * 255.0f));
        canvas->drawImage(img.get(), 0.0f, 0.0f, SkSamplingOptions(SkFilterMode::kLinear), &paint);
    }
}

} // extern "C"
```

## 3. Calling It from `renderFrame()` in `main.cpp`
Now Step 4 of your `main.cpp` frame loop collapses to a single FFI call before snapping the Skia Graphite recording:

```cpp
// Inside renderFrame():
tab.hermesRuntime->drainMicrotasks();

SkCanvas* domCanvas = tab.skiaSurface->getCanvas();
domCanvas->clear(SK_ColorWHITE);

// Resolves Stylo + Taffy + Parley and streams draw calls into Skia Graphite
blitz_doc_paint_to_skia(
    tab.blitzDoc,
    static_cast<void*>(domCanvas),
    tab.width,
    tab.height,
    1.0 // HiDPI scale factor
);

// Snap Skia Graphite recording & submit to the shared Dawn queue
std::unique_ptr<skgpu::graphite::Recording> recording = gpu.skiaRecorder->snap();
if (recording) {
    skgpu::graphite::InsertRecordingInfo insertInfo{};
    insertInfo.fRecording = recording.get();
    gpu.skiaContext->insertRecording(insertInfo);
    gpu.skiaContext->submit(skgpu::graphite::SyncToCpu::kNo);
}
```

## Two Details That Keep This Fast
- **`save()` / `restore()` stack balance in `push_layer`:** Notice that in `skia_bridge_push_layer`, calling `canvas->saveLayer()` followed by `canvas->concat()` and `canvas->clipRect()` only pushes one state frame onto Skia's internal stack. When `blitz-paint` finishes painting that stacking context or overflow container and calls `pop_layer()`, a single `canvas->restore()` pops both the clip/matrix transform and composites the offscreen alpha layer in one step.
- **Zero text re-shaping in C++:** Because `blitz-dom` caches Parley's shaped glyph runs inside the Rust DOM nodes and only invalidates paragraphs whose text or width actually changed during `resolve()`, C++ never runs `SkParagraph` or HarfBuzz. It simply `memcpy`s the pre-positioned glyph IDs and `(x, y)` coordinates straight into `SkTextBlobBuilder::allocRunPos`, which Skia Graphite uploads directly to its GPU glyph atlas on your shared Dawn device.

## 1. Project Layout & `Cargo.toml`

```text
hermes-blitz-browser/
├── Cargo.toml
├── build.rs
├── c_shim/
│   └── hermes_shim.cpp      # C-ABI wrapper around SHRuntime & sh_export_dom_runtime
└── src/
    ├── js/
    │   └── dom_runtime.ts   # Compiled by shermes -typed -emit-c in build.rs
    ├── dom_ffi.rs           # #[no_mangle] extern "C" Blitz DOM callbacks
    └── main.rs              # Winit + wgpu + anyrender_vello + Hermes event loop
```

```toml
[package]
name = "hermes-blitz-browser"
version = "0.1.0"
edition = "2024"

[dependencies]
blitz-dom = "0.2"
blitz-html = "0.2"
blitz-paint = "0.2"
blitz-traits = "0.2"
anyrender = "0.2"
anyrender_vello = "0.2"
wgpu = "24.0"
winit = "0.30"
pollster = "0.4"

[build-dependencies]
cc = "1.2"
```

---

To embed Static Hermes (`static_h`) into a pure Rust Blitz + Vello + `wgpu` application, you compile your TypeScript DOM runtime into C source via `shermes -typed -emit-c`, wrap the generated `SHUnit` initialization and Hermes `facebook::jsi::Runtime` hooks in a 60-line C++ shim, and link everything into Cargo via `build.rs`.

At runtime, Winit manages the window and mouse events, Blitz manages the HTML/CSS DOM (`HtmlDocument`), Static Hermes executes the ahead-of-time compiled `SHUnit` (calling back into Rust's `#[no_mangle] pub extern "C"` functions via `$SHBuiltin.extern_c`), and `anyrender_vello` renders the resolved layout directly onto the `wgpu::Surface`.

## 2. The C++ Shim Around Static Hermes (`c_shim/hermes_shim.cpp`)
When `shermes -typed -exported-unit=dom_runtime -emit-c` compiles `dom_runtime.ts`, it emits a C function `SHUnit *sh_export_dom_runtime(void)`. In Static Hermes (`static_h`), `SHRuntime` and `facebook::hermes::HermesRuntime` share the same underlying VM instance—meaning you can load your ahead-of-time compiled native `SHUnit` for zero-cost `$SHBuiltin.extern_c` DOM calls, while still using `facebook::jsi::Runtime` to invoke global functions or evaluate dynamic `<script>` strings downloaded from the web.

```cpp
// c_shim/hermes_shim.cpp
#include <hermes/VM/static_h.h>
#include <hermes/hermes.h>
#include <jsi/jsi.h>
#include <cstdint>

using namespace facebook;

// Emitted by: shermes -typed -exported-unit=dom_runtime -emit-c dom_runtime.ts
extern "C" SHUnit* sh_export_dom_runtime(void);

struct HermesAppRuntime {
    SHRuntime* shRuntime;
    hermes::HermesRuntime* jsiRuntime;
};

extern "C" {

HermesAppRuntime* hermes_app_init(void* blitz_ctx_ptr) {
    auto* app = new HermesAppRuntime();

    // 1. Initialize Static Hermes SHRuntime (backed by HermesRuntime)
    app->shRuntime = _sh_init(0, nullptr);
    app->jsiRuntime = reinterpret_cast<hermes::HermesRuntime*>(
        _sh_get_hermes_runtime(app->shRuntime)
    );

    // 2. Register and execute the AOT-compiled dom_runtime unit
    SHLegacyValue unitRes = _sh_unit_init_guarded(
        app->shRuntime, sh_export_dom_runtime
    );
    (void)unitRes;

    // 3. Pass the Rust BlitzHostContext* pointer into globalThis.__initBlitzContext
    jsi::Runtime& rt = *app->jsiRuntime;
    jsi::Function initFn = rt.global()
        .getPropertyAsFunction(rt, "__initBlitzContext");

    // In Static Hermes, c_ptr is represented as a 64-bit IEEE-754 double / SHLegacyValue
    double rawPtrAsNumber = static_cast<double>(
        reinterpret_cast<uintptr_t>(blitz_ctx_ptr)
    );
    initFn.call(rt, jsi::Value(rawPtrAsNumber));

    return app;
}

void hermes_app_dispatch_event(
    HermesAppRuntime* app,
    const char* event_type,
    size_t hit_node_id,
    float x,
    float y
) {
    jsi::Runtime& rt = *app->jsiRuntime;
    jsi::Function dispatchFn = rt.global()
        .getPropertyAsFunction(rt, "__dispatchBlitzEvent");

    dispatchFn.call(
        rt,
        jsi::String::createFromUtf8(rt, event_type),
        jsi::Value(static_cast<double>(hit_node_id)),
        jsi::Value(static_cast<double>(x)),
        jsi::Value(static_cast<double>(y))
    );

    // Drain Promise/async microtasks queued by event listeners
    rt.drainMicrotasks();
}

void hermes_app_eval_script(
    HermesAppRuntime* app,
    const char* script_utf8,
    const char* source_url
) {
    jsi::Runtime& rt = *app->jsiRuntime;
    auto buf = std::make_shared<jsi::StringBuffer>(script_utf8);
    rt.evaluateJavaScript(buf, source_url);
    rt.drainMicrotasks();
}

void hermes_app_destroy(HermesAppRuntime* app) {
    if (!app) return;
    _sh_done(app->shRuntime);
    delete app;
}

} // extern "C"
```

## 3. `build.rs`: Running `shermes` and Linking `libhermesvm`
The Cargo build script invokes `shermes` to transpile `src/js/dom_runtime.ts` into C, compiles the generated C file alongside `c_shim/hermes_shim.cpp`, and links the Static Hermes libraries.

```rust
// build.rs
use std::{env, path::PathBuf, process::Command};

fn main() {
    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let hermes_src = PathBuf::from(
        env::var("HERMES_SRC_DIR").expect("Set HERMES_SRC_DIR to facebook/hermes checkout")
    );
    let hermes_build = PathBuf::from(
        env::var("HERMES_BUILD_DIR").expect("Set HERMES_BUILD_DIR to cmake build dir")
    );

    let shermes_bin = hermes_build.join("bin/shermes");
    let ts_input = PathBuf::from("src/js/dom_runtime.ts");
    let generated_c = out_dir.join("dom_runtime.c");

    println!("cargo:rerun-if-changed=src/js/dom_runtime.ts");
    println!("cargo:rerun-if-changed=c_shim/hermes_shim.cpp");

    // 1. Compile TypeScript DOM bindings + page logic to C via shermes
    let status = Command::new(&shermes_bin)
        .args([
            "-typed",
            "-exported-unit=dom_runtime",
            "-emit-c",
            "-O",
            ts_input.to_str().unwrap(),
            "-o",
            generated_c.to_str().unwrap(),
        ])
        .status()
        .expect("Failed to execute shermes");
    assert!(status.success(), "shermes compilation failed");

    // 2. Compile generated dom_runtime.c (C11)
    cc::Build::new()
        .file(&generated_c)
        .include(hermes_src.join("include"))
        .include(hermes_src.join("API"))
        .include(hermes_build.join("lib/config"))
        .flag_if_supported("-Wno-unused-Variable")
        .flag_if_supported("-Wno-unused-function")
        .compile("dom_runtime_unit");

    // 3. Compile C++ shim (C++20)
    cc::Build::new()
        .cpp(true)
        .std("c++20")
        .file("c_shim/hermes_shim.cpp")
        .include(hermes_src.join("include"))
        .include(hermes_src.join("API"))
        .include(hermes_src.join("API/jsi"))
        .include(hermes_src.join("public"))
        .include(hermes_build.join("lib/config"))
        .compile("hermes_shim");

    // 4. Link Static Hermes VM and JSI libraries
    println!("cargo:rustc-link-search=native={}/lib", hermes_build.display());
    println!("cargo:rustc-link-search=native={}/API/hermes", hermes_build.display());
    println!("cargo:rustc-link-search=native={}/API/jsi/jsi", hermes_build.display());
    println!("cargo:rustc-link-lib=dylib=hermesvm");
    println!("cargo:rustc-link-lib=static=jsi");
}
```

## 4. Rust Main Loop: `winit` + `wgpu` + `anyrender_vello` + Blitz (`src/main.rs`)
Now the Rust binary boots the `wgpu` device, parses the initial HTML/CSS document with `blitz-html`, initializes Static Hermes with a pointer to `BlitzHostContext`, and renders the scene into the window swapchain texture via `anyrender_vello::VelloWindowRenderer`.

```rust
mod dom_ffi;

use anyrender::WindowRenderer;
use anyrender_vello::VelloWindowRenderer;
use blitz_dom::DocumentConfig;
use blitz_html::HtmlDocument;
use blitz_traits::shell::{ColorScheme, Viewport};
use dom_ffi::BlitzHostContext;
use std::ffi::{CString, c_char, c_void};
use std::sync::Arc;
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, EventLoop};
use winit::window::{Window, WindowId};

#[repr(C)]
pub struct HermesAppRuntime {
    _private: [u8; 0],
}

unsafe extern "C" {
    fn hermes_app_init(blitz_ctx_ptr: *mut c_void) -> *mut HermesAppRuntime;
    fn hermes_app_dispatch_event(
        app: *mut HermesAppRuntime,
        event_type: *const c_char,
        hit_node_id: usize,
        x: f32,
        y: f32,
    );
    fn hermes_app_eval_script(
        app: *mut HermesAppRuntime,
        script_utf8: *const c_char,
        source_url: *const c_char,
    );
    fn hermes_app_destroy(app: *mut HermesAppRuntime);
}

const INITIAL_HTML: &str = r#"
<!DOCTYPE html>
<html>
<head>
  <style>
    body {
      background-color: #0F172A;
      color: #F8FAFC;
      font-family: sans-serif;
      padding: 32px;
      display: flex;
      flex-direction: column;
      gap: 16px;
    }
    .card {
      background-color: #1E293B;
      border: 1px solid #334155;
      border-radius: 12px;
      padding: 24px;
      display: flex;
      flex-direction: column;
      gap: 12px;
    }
    #action-btn {
      background-color: #2563EB;
      color: #FFFFFF;
      padding: 12px 20px;
      border-radius: 8px;
      width: 220px;
    }
  </style>
</head>
<body>
  <div class="card">
    <h1>Static Hermes + Blitz + Vello</h1>
    <p id="status-label">Click the button below to test $SHBuiltin.extern_c DOM mutation.</p>
    <div id="action-btn">Increment Counter (0)</div>
  </div>
</body>
</html>
"#;

struct BrowserApp {
    window: Option<Arc<Window>>,
    renderer: VelloWindowRenderer,
    blitz_ctx: Box<BlitzHostContext>,
    hermes_rt: *mut HermesAppRuntime,
    cursor_pos: (f32, f32),
    width: u32,
    height: u32,
    scale: f64,
}

impl BrowserApp {
    fn new() -> Self {
        let width = 1024;
        let height = 768;
        let scale = 1.0;

        // 1. Parse initial HTML & CSS via blitz-html (html5ever + Stylo)
        let viewport = Viewport::new(width, height, scale as f32, ColorScheme::Dark);
        let mut doc = HtmlDocument::from_html(
            INITIAL_HTML,
            DocumentConfig {
                viewport: Some(viewport),
                ..Default::default()
            },
        );
        doc.as_mut().resolve(0.0);

        let mut blitz_ctx = Box::new(BlitzHostContext::new(doc));

        // 2. Boot Static Hermes & wire globalBlitzCtx to our Rust BlitzHostContext*
        let hermes_rt = unsafe {
            hermes_app_init((&mut *blitz_ctx as *mut BlitzHostContext).cast::<c_void>())
        };

        Self {
            window: None,
            renderer: VelloWindowRenderer::new(),
            blitz_ctx,
            hermes_rt,
            cursor_pos: (0.0, 0.0),
            width,
            height,
            scale,
        }
    }

    fn render_current_frame(&mut self) {
        if !self.renderer.is_active() {
            return;
        }
        let doc_ref = self.blitz_ctx.doc.as_ref();
        let scale = self.scale;
        let width = self.width;
        let height = self.height;

        // Render Blitz DOM tree directly to Vello's wgpu compute pipeline
        self.renderer.render(|scene_painter| {
            blitz_paint::paint_scene(scene_painter, doc_ref, scale, width, height);
        });
    }
}

impl ApplicationHandler for BrowserApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            let attrs = Window::default_attributes()
                .with_title("Static Hermes + Blitz + Vello Browser")
                .with_inner_size(PhysicalSize::new(self.width, self.height));
            let window = Arc::new(event_loop.create_window(attrs).unwrap());
            self.scale = window.scale_factor();
            self.renderer.resume(window.clone(), self.width, self.height);
            self.window = Some(window);
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.renderer.suspend();
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),

            WindowEvent::Resized(physical_size) => {
                self.width = physical_size.width.max(1);
                self.height = physical_size.height.max(1);
                self.renderer.set_size(self.width, self.height);

                let viewport = Viewport::new(
                    self.width,
                    self.height,
                    self.scale as f32,
                    ColorScheme::Dark,
                );
                self.blitz_ctx.doc.as_mut().set_viewport(viewport);
                self.blitz_ctx.doc.as_mut().resolve(0.0);
                if let Some(w) = &self.window {
                    w.request_redraw();
                }
            }

            WindowEvent::CursorMoved { position, .. } => {
                // Convert physical cursor coordinates to logical CSS pixels
                self.cursor_pos = (
                    (position.x / self.scale) as f32,
                    (position.y / self.scale) as f32,
                );
            }

            WindowEvent::MouseInput {
                state: ElementState::Pressed,
                button: MouseButton::Left,
                ..
            } => {
                let (x, y) = self.cursor_pos;
                // 1. Hit-test in Blitz (Stylo + Taffy layout coordinates)
                if let Some(hit) = self.blitz_ctx.doc.as_ref().hit(x, y) {
                    // 2. Dispatch "click" into Static Hermes event bubble loop
                    unsafe {
                        hermes_app_dispatch_event(
                            self.hermes_rt,
                            c"click".as_ptr(),
                            hit.node_id,
                            x,
                            y,
                        );
                    }
                    // 3. If JS mutated the DOM via $SHBuiltin.extern_c, re-resolve Stylo/Taffy
                    if self.blitz_ctx.dirty {
                        self.blitz_ctx.doc.as_mut().resolve(0.0);
                        self.blitz_ctx.dirty = false;
                        if let Some(w) = &self.window {
                            w.request_redraw();
                        }
                    }
                }
            }

            WindowEvent::RedrawRequested => {
                self.render_current_frame();
            }

            _ => {}
        }
    }
}

impl Drop for BrowserApp {
    fn drop(&mut self) {
        unsafe {
            hermes_app_destroy(self.hermes_rt);
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().unwrap();
    let mut app = BrowserApp::new();
    event_loop.run_app(&mut app).unwrap();
}
```

## How the Execution Lifecycle Flows at Runtime
- **Startup:** `BrowserApp::new()` parses `INITIAL_HTML` into Blitz's slab arena (`Slab<Node>`) and calls `hermes_app_init()`, which runs the AOT-compiled `sh_export_dom_runtime` unit and binds the `#action-btn` click listener in JS.
- **Click Event:** Clicking the window fires `WindowEvent::MouseInput`. Rust calls `self.blitz_ctx.doc.as_ref().hit(x, y)` in sub-microsecond time to get the leaf `NodeId` and calls `hermes_app_dispatch_event`.
- **Zero-Cost DOM Mutation:** The JS callback in `dom_runtime.ts` invokes `btn.style.setProperty(...)` and `label.innerText = ...`, which execute the `$SHBuiltin.extern_c` symbols `blitz_set_style_property` and `blitz_set_inner_text` directly in Rust, mutating `DocumentMutator` and setting `self.blitz_ctx.dirty = true`.
- **Incremental Relayout & GPU Paint:** Returning to `WindowEvent::MouseInput`, Rust sees `self.blitz_ctx.dirty == true`, runs `doc.resolve(0.0)` (recomputing only damaged Stylo/Taffy nodes), and requests a redraw where `anyrender_vello` dispatches Vello compute shaders onto the `wgpu` queue.
