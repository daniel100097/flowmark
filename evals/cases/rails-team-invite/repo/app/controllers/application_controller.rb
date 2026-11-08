class ApplicationController < ActionController::API
  include Pundit::Authorization
  before_action :authenticate_user!

  rescue_from Pundit::NotAuthorizedError do
    head :forbidden
  end
end
