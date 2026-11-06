pluginManagement {
  repositories {
    google {
      content {
        includeGroupByRegex("androidx.*")
        includeGroupByRegex("com\\.android.*")
        includeGroupByRegex("com\\.google.*")
      }
    }
    mavenCentral()
    gradlePluginPortal()
  }
}

val localProperties = java.util.Properties().apply {
  val file = rootDir.resolve("local.properties")
  if (file.exists()) file.inputStream().use(::load)
}

dependencyResolutionManagement {
  repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
  repositories {
    google {
      content {
        includeGroupByRegex("androidx.*")
        includeGroupByRegex("com\\.android.*")
        includeGroupByRegex("com\\.google.*")
      }
    }
    mavenCentral()
    maven {
      url = uri("https://maven.pkg.github.com/facebook/meta-wearables-dat-android")
      credentials {
        username = System.getenv("GITHUB_ACTOR")
          ?: localProperties.getProperty("github_user")
          ?: "maceip"
        password = System.getenv("MWDAT_GITHUB_TOKEN")
          ?: System.getenv("GITHUB_TOKEN")
          ?: localProperties.getProperty("github_token", "")
      }
    }
  }
}

plugins {
  id("org.gradle.toolchains.foojay-resolver-convention") version "1.0.0"
}

rootProject.name = "Walkaway Bridge"

include(":app")
